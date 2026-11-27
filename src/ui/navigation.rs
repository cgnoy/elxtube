#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Home,
    Subscription,
    History,
    Search(String),
    Channel(String),
    Watch(String),
    Playlist(String, String),
    PlaylistInput,
}

impl Route {
    pub fn page_name(&self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Subscription => "subs",
            Self::History => "history",
            Self::Search(_) => "search",
            Self::Channel(_) => "channel",
            Self::Watch(_) => "player",
            Self::Playlist(_, _) | Self::PlaylistInput => "playlist",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NavigationStack {
    entries: Vec<Route>,
}

impl NavigationStack {
    pub fn new(initial: Route) -> Self {
        Self {
            entries: vec![initial],
        }
    }

    pub fn current(&self) -> &Route {
        self.entries.last().expect("navigation stack is not empty")
    }

    pub fn push(&mut self, route: Route) {
        if self.current() != &route {
            self.entries.push(route);
        }
    }

    pub fn back(&mut self) -> bool {
        if self.entries.len() <= 1 {
            return false;
        }
        self.entries.pop();
        true
    }

    #[cfg(test)]
    pub fn depth(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_preserves_context_stack() {
        let mut navigation = NavigationStack::new(Route::Home);
        navigation.push(Route::Search("rust".into()));
        navigation.push(Route::Watch("abc".into()));
        assert_eq!(navigation.depth(), 3);
        assert!(navigation.back());
        assert_eq!(navigation.current(), &Route::Search("rust".into()));
        assert!(navigation.back());
        assert_eq!(navigation.current(), &Route::Home);
        assert!(!navigation.back());
        navigation.push(Route::PlaylistInput);
        assert_eq!(navigation.current().page_name(), "playlist");
        assert!(navigation.back());
    }
}
