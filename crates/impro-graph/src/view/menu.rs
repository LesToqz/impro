//! The node context menu and the graph-type menu.

use gtk::prelude::*;

/// Which graph the view shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GraphType {
    /// Every file of the project.
    #[default]
    Global,
    /// The active file and its neighbourhood.
    Local,
    /// Only the files in the notes folder.
    Note,
}

impl GraphType {
    pub const ALL: [GraphType; 3] = [GraphType::Global, GraphType::Local, GraphType::Note];

    /// The `graph_default_type` setting value.
    pub fn as_str(self) -> &'static str {
        match self {
            GraphType::Global => "global",
            GraphType::Local => "local",
            GraphType::Note => "note",
        }
    }

    pub fn parse(text: &str) -> Option<GraphType> {
        GraphType::ALL.into_iter().find(|t| t.as_str() == text)
    }

    pub fn label(self) -> &'static str {
        match self {
            GraphType::Global => "Global graph",
            GraphType::Local => "Local graph",
            GraphType::Note => "Note graph",
        }
    }

    pub fn tip(self) -> &'static str {
        match self {
            GraphType::Global => "Every file in the project",
            GraphType::Local => "The active file and its neighbours",
            GraphType::Note => "Only the files in the notes folder",
        }
    }
}

/// Actions of the node context menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextAction {
    Open,
    OpenNewTab,
    OpenRightPane,
    Reveal,
    CopyPath,
}

/// Menu entries in display order; a separator follows the third.
pub const CONTEXT_ITEMS: [(&str, ContextAction); 5] = [
    ("Open file", ContextAction::Open),
    ("Open another tab", ContextAction::OpenNewTab),
    ("Open in right pane", ContextAction::OpenRightPane),
    ("Locate in project tree", ContextAction::Reveal),
    ("Copy file location", ContextAction::CopyPath),
];

/// Builds the context menu of a file marker, titled with its label.
pub fn context_menu(title: &str, on_action: impl Fn(ContextAction) + 'static) -> gtk::Menu {
    let menu = gtk::Menu::new();
    menu.style_context().add_class("impro-graph-menu");
    let header = gtk::MenuItem::with_label(title);
    header.set_sensitive(false);
    header.style_context().add_class("impro-graph-menu-title");
    menu.append(&header);
    let on_action = std::rc::Rc::new(on_action);
    for (i, (text, action)) in CONTEXT_ITEMS.iter().enumerate() {
        if i == 3 {
            let sep = gtk::SeparatorMenuItem::new();
            sep.style_context().add_class("impro-graph-menu-separator");
            menu.append(&sep);
        }
        let item = gtk::MenuItem::with_label(text);
        item.style_context().add_class("impro-graph-menu-item");
        let action = *action;
        let on_action = on_action.clone();
        item.connect_activate(move |_| on_action(action));
        menu.append(&item);
    }
    menu.show_all();
    menu
}

/// Builds the graph-type menu with `current` marked.
pub fn graph_type_menu(current: GraphType, on_pick: impl Fn(GraphType) + 'static) -> gtk::Menu {
    let menu = gtk::Menu::new();
    menu.style_context().add_class("impro-graph-menu");
    let on_pick = std::rc::Rc::new(on_pick);
    let mut group: Option<gtk::RadioMenuItem> = None;
    for t in GraphType::ALL {
        let item = match &group {
            None => gtk::RadioMenuItem::with_label(t.label()),
            Some(first) => gtk::RadioMenuItem::with_label_from_widget(first, Some(t.label())),
        };
        item.set_tooltip_text(Some(t.tip()));
        item.style_context().add_class("impro-graph-menu-item");
        item.set_active(t == current);
        let on_pick = on_pick.clone();
        item.connect_activate(move |i| {
            if i.is_active() && t != current {
                on_pick(t);
            }
        });
        menu.append(&item);
        group.get_or_insert(item);
    }
    menu.show_all();
    menu
}

/// The activatable entries of a context menu, in order.
pub fn activatable_labels(menu: &gtk::Menu) -> Vec<String> {
    menu.children()
        .into_iter()
        .filter_map(|c| c.downcast::<gtk::MenuItem>().ok())
        .filter(|i| i.is_sensitive() && i.downcast_ref::<gtk::SeparatorMenuItem>().is_none())
        .filter_map(|i| i.label().map(|l| l.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_types_round_trip() {
        for t in GraphType::ALL {
            assert_eq!(GraphType::parse(t.as_str()), Some(t));
        }
        assert_eq!(GraphType::parse("other"), None);
        assert_eq!(GraphType::default(), GraphType::Global);
    }

    #[test]
    fn context_entries_are_exactly_the_documented_five() {
        let labels: Vec<&str> = CONTEXT_ITEMS.iter().map(|i| i.0).collect();
        assert_eq!(
            labels,
            [
                "Open file",
                "Open another tab",
                "Open in right pane",
                "Locate in project tree",
                "Copy file location"
            ]
        );
    }
}
