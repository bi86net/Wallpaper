use crate::service::{self, Category, Wallpaper};
use rand::{Rng, seq::SliceRandom};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::mpsc::{self, Sender},
    time::Duration,
};
#[derive(Clone, Debug, PartialEq)]
pub struct Schedule {
    pub enabled: bool,
    pub minutes: u32,
    pub categories: Vec<Category>,
}
impl Schedule {
    pub fn duration(&self) -> Duration {
        Duration::from_secs(u64::from(self.minutes.clamp(1, 1440)) * 60)
    }
}
pub enum Command {
    Configure(Schedule),
    Previous,
    Next,
}
#[derive(Default)]
struct History {
    paths: Vec<PathBuf>,
    position: Option<usize>,
}
impl History {
    fn previous(&self) -> Option<usize> {
        self.position.and_then(|p| p.checked_sub(1))
    }
    fn next(&self) -> Option<usize> {
        self.position
            .map(|p| p + 1)
            .filter(|p| *p < self.paths.len())
    }
    fn discard_forward(&mut self) {
        self.paths.truncate(self.position.map_or(0, |p| p + 1));
    }
    fn push(&mut self, path: PathBuf) {
        if self.position.is_some_and(|p| self.paths[p] == path) {
            return;
        }
        self.paths.truncate(self.position.map_or(0, |p| p + 1));
        self.paths.push(path);
        self.position = Some(self.paths.len() - 1);
    }
}
struct Feed {
    category: Category,
    page: u32,
    index: usize,
    last_page: u32,
    items: Vec<Wallpaper>,
}
impl Feed {
    fn new(category: Category) -> Self {
        Self {
            category,
            page: 1,
            index: 0,
            last_page: 1,
            items: Vec::new(),
        }
    }
    fn prepare(&mut self, history: &History, random: bool) -> anyhow::Result<PathBuf> {
        if self.index >= self.items.len() {
            let response =
                service::search(self.category, if random { 1 } else { self.page }, random)?;
            self.items = response.data;
            if random {
                self.items.shuffle(&mut rand::thread_rng());
            }
            self.last_page = response.meta.last_page.max(1);
            self.index = 0;
            anyhow::ensure!(
                !self.items.is_empty(),
                "{}分类没有可用的 4K 壁纸",
                self.category.label()
            );
        }
        loop {
            let path = service::download(&self.items[self.index])?;
            if history.position.is_some_and(|p| history.paths[p] == path)
                && self.index + 1 < self.items.len()
            {
                self.index += 1;
            } else {
                return Ok(path);
            }
        }
    }
    fn advance(&mut self) {
        self.index += 1;
        if self.index >= self.items.len() {
            self.page = if self.page >= self.last_page {
                1
            } else {
                self.page + 1
            };
        }
    }
}
struct Feeds {
    items: Vec<Feed>,
    next: usize,
}
impl Feeds {
    fn new(categories: &[Category]) -> Self {
        let mut unique = Vec::new();
        for category in categories {
            if !unique.contains(category) {
                unique.push(*category);
            }
        }
        if unique.is_empty() {
            unique.push(Category::Landscape);
        }
        Self {
            items: unique.into_iter().map(Feed::new).collect(),
            next: 0,
        }
    }
    fn choose(&mut self, rng: &mut impl Rng) {
        self.next = rng.gen_range(0..self.items.len());
    }
    fn advance(&mut self) {
        self.items[self.next].advance();
    }
}
// All desktop changes are serialized here; history advances only after successful application.
pub fn start(events: Sender<crate::Event>, ctx: eframe::egui::Context) -> Sender<Command> {
    let (tx, rx) = mpsc::channel::<Command>();
    std::thread::spawn(move || {
        let mut schedule = Schedule {
            enabled: false,
            minutes: 5,
            categories: vec![Category::Landscape],
        };
        let mut feeds = Feeds::new(&schedule.categories);
        let mut history = History::default();
        let mut pending = VecDeque::new();
        loop {
            let command = if let Some(c) = pending.pop_front() {
                c
            } else {
                let received = if schedule.enabled {
                    rx.recv_timeout(schedule.duration())
                } else {
                    rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                };
                match received {
                    Ok(c) => c,
                    Err(mpsc::RecvTimeoutError::Timeout) => Command::Next,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            };
            if let Command::Configure(next) = command {
                if schedule.categories != next.categories {
                    feeds = Feeds::new(&next.categories);
                    history.discard_forward();
                }
                schedule = next;
                continue;
            }
            let _ = events.send(crate::Event::RotationStarted);
            ctx.request_repaint();
            let mut target = None;
            let mut advance = false;
            let result = (|| -> anyhow::Result<PathBuf> {
                match command {
                    Command::Previous => {
                        let p = history
                            .previous()
                            .ok_or_else(|| anyhow::anyhow!("已经是第一张壁纸"))?;
                        target = Some(p);
                        Ok(history.paths[p].clone())
                    }
                    Command::Next => {
                        if let Some(p) = history.next() {
                            target = Some(p);
                            return Ok(history.paths[p].clone());
                        }
                        feeds.choose(&mut rand::thread_rng());
                        let random = feeds.items.len() > 1;
                        let path = feeds.items[feeds.next].prepare(&history, random)?;
                        advance = true;
                        Ok(path)
                    }
                    Command::Configure(_) => unreachable!(),
                }
            })();
            pending.extend(rx.try_iter());
            let result = if pending.iter().any(|c| matches!(c, Command::Configure(_))) {
                Ok("设置已更新，本次切换已取消".into())
            } else {
                result
                    .and_then(|path| {
                        wallpaper::set_from_path(
                            path.to_str()
                                .ok_or_else(|| anyhow::anyhow!("路径编码错误"))?,
                        )
                        .map_err(|e| anyhow::anyhow!("设置壁纸失败：{e}"))?;
                        if let Some(p) = target {
                            history.position = Some(p);
                        } else {
                            history.push(path.clone());
                        }
                        if advance {
                            feeds.advance();
                        }
                        Ok(format!("已更换壁纸：{}", path.display()))
                    })
                    .map_err(|e| format!("更换失败：{e:#}"))
            };
            if events
                .send(crate::Event::Rotation(result, history.previous().is_some()))
                .is_err()
            {
                return;
            }
            ctx.request_repaint();
        }
    });
    tx
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiple_categories_are_random_instead_of_alternating() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(260911);
        let mut feeds = Feeds::new(&[Category::Landscape, Category::Beauty]);
        let mut choices = Vec::new();
        for _ in 0..64 {
            feeds.choose(&mut rng);
            choices.push(feeds.next);
            feeds.advance();
        }
        assert!(choices.contains(&0) && choices.contains(&1));
        assert!(choices.windows(2).any(|pair| pair[0] == pair[1]));
        assert_eq!(
            feeds.items[0].index,
            choices.iter().filter(|i| **i == 0).count()
        );
        assert_eq!(
            feeds.items[1].index,
            choices.iter().filter(|i| **i == 1).count()
        );
    }
    #[test]
    fn one_category_stays_selected_and_empty_input_is_safe() {
        let mut feeds = Feeds::new(&[Category::Beauty]);
        feeds.choose(&mut rand::thread_rng());
        feeds.advance();
        assert_eq!(feeds.items[feeds.next].category, Category::Beauty);
        assert_eq!(Feeds::new(&[]).items[0].category, Category::Landscape);
        assert_eq!(
            Feeds::new(&[Category::Beauty, Category::Beauty])
                .items
                .len(),
            1
        );
    }
    #[test]
    fn category_change_discards_old_forward_items_but_keeps_previous() {
        let mut history = History::default();
        for name in ["a", "b", "c"] {
            history.push(name.into());
        }
        history.position = Some(1);
        history.discard_forward();
        assert_eq!(history.next(), None);
        assert_eq!(history.previous(), Some(0));
        assert_eq!(history.paths.len(), 2);
    }
    #[test]
    fn history_navigates_both_directions() {
        let mut history = History::default();
        assert_eq!(history.previous(), None);
        assert_eq!(history.next(), None);
        history.push("a.jpg".into());
        history.push("b.jpg".into());
        assert_eq!(history.previous(), Some(0));
        assert_eq!(history.next(), None);
        history.position = history.previous();
        assert_eq!(history.next(), Some(1));
        assert_eq!(
            history.paths[history.next().unwrap()],
            PathBuf::from("b.jpg")
        );
    }
    #[test]
    fn new_selection_replaces_forward_history_and_ignores_duplicates() {
        let mut history = History::default();
        history.push("a.jpg".into());
        history.push("b.jpg".into());
        history.position = Some(0);
        history.push("c.jpg".into());
        assert_eq!(
            history.paths,
            vec![PathBuf::from("a.jpg"), PathBuf::from("c.jpg")]
        );
        history.push("c.jpg".into());
        assert_eq!(history.paths.len(), 2);
        assert_eq!(history.position, Some(1));
    }
    #[test]
    fn preset_and_custom_intervals() {
        for n in [1, 5, 10, 37] {
            assert_eq!(
                Schedule {
                    enabled: true,
                    minutes: n,
                    categories: vec![Category::Beauty]
                }
                .duration()
                .as_secs(),
                u64::from(n) * 60
            );
        }
    }
    #[test]
    fn invalid_intervals_are_bounded() {
        for (n, seconds) in [(0, 60), (u32::MAX, 86400)] {
            assert_eq!(
                Schedule {
                    enabled: true,
                    minutes: n,
                    categories: vec![Category::Landscape]
                }
                .duration()
                .as_secs(),
                seconds
            );
        }
    }
}
