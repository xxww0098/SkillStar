//! Types and filter enums for MySkills page.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MySkillsScope {
    #[default]
    Local,
    Remote,
    Channels,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SortOption {
    #[default]
    Updated,
    Name,
    Stars,
}

impl SortOption {
    pub const ALL: [SortOption; 3] = [SortOption::Updated, SortOption::Name, SortOption::Stars];

    pub fn label(self) -> gpui_kit::SharedString {
        crate::i18n::t(match self {
            SortOption::Updated => "toolbar.updated",
            SortOption::Name => "toolbar.sortName",
            SortOption::Stars => "toolbar.sortStars",
        })
    }

    pub fn next(self) -> Self {
        match self {
            SortOption::Updated => SortOption::Name,
            SortOption::Name => SortOption::Stars,
            SortOption::Stars => SortOption::Updated,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SourceFilter {
    #[default]
    All,
    Hub,
    Local,
}

impl SourceFilter {
    pub const ALL: [SourceFilter; 3] = [SourceFilter::All, SourceFilter::Hub, SourceFilter::Local];

    pub fn label(self) -> gpui_kit::SharedString {
        crate::i18n::t(match self {
            SourceFilter::All => "toolbar.all",
            SourceFilter::Hub => "toolbar.hub",
            SourceFilter::Local => "toolbar.local",
        })
    }
}
