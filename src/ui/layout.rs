#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarMode {
    Expanded,
    Rail,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutSpec {
    pub breakpoint: u8,
    pub sidebar: SidebarMode,
    pub sidebar_width: i32,
    pub columns: u32,
    pub compact: bool,
    pub watch_compact: bool,
}

impl LayoutSpec {
    pub fn for_width(width: i32) -> Self {
        if width >= 1600 {
            Self {
                breakpoint: 3,
                sidebar: SidebarMode::Expanded,
                sidebar_width: 240,
                columns: 3,
                compact: false,
                watch_compact: false,
            }
        } else if width >= 1180 {
            Self {
                breakpoint: 2,
                sidebar: SidebarMode::Expanded,
                sidebar_width: 240,
                columns: 3,
                compact: false,
                watch_compact: false,
            }
        } else if width >= 640 {
            Self {
                breakpoint: 1,
                sidebar: SidebarMode::Rail,
                sidebar_width: 72,
                columns: 3,
                compact: true,
                watch_compact: true,
            }
        } else {
            Self {
                breakpoint: 0,
                sidebar: SidebarMode::Rail,
                sidebar_width: 72,
                columns: 1,
                compact: true,
                watch_compact: true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoints_match_target_screens() {
        assert_eq!(LayoutSpec::for_width(1920).columns, 3);
        assert_eq!(LayoutSpec::for_width(1366).columns, 3);
        assert_eq!(LayoutSpec::for_width(1024).sidebar, SidebarMode::Rail);
        assert_eq!(LayoutSpec::for_width(900).columns, 3);
        assert_eq!(LayoutSpec::for_width(1100).columns, 3);
        assert_eq!(LayoutSpec::for_width(640).sidebar, SidebarMode::Rail);
        assert_eq!(LayoutSpec::for_width(639).sidebar, SidebarMode::Rail);
        assert!(LayoutSpec::for_width(1024).watch_compact);
    }
}
