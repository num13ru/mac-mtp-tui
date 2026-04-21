use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Host,
    Device,
}

pub struct ConfirmDialog {
    pub title: String,
    pub message: String,
    pub on_confirm: ConfirmAction,
}

pub enum ConfirmAction {
    OverwritePush {
        source: PathBuf,
        delete_id: String,
    },
    OverwritePull {
        entry_id: String,
        filename: String,
    },
    Delete {
        entry_id: String,
        name: String,
    },
}

pub struct TextInputDialog {
    pub title: String,
    pub prompt: String,
    pub input: String,
    pub cursor_pos: usize,
    pub on_submit: TextInputAction,
}

pub enum TextInputAction {
    Mkdir,
    Rename { entry_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone)]
pub struct HostEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct DeviceEntry {
    pub id: String,
    pub name: String,
    pub kind: DeviceEntryKind,
    pub size: Option<u64>,
}

pub struct PaneState<T> {
    pub entries: Vec<T>,
    pub selected: usize,
}

impl<T> PaneState<T> {
    pub fn new(entries: Vec<T>) -> Self {
        Self {
            entries,
            selected: 0,
        }
    }

    pub fn select_next(&mut self) {
        if self.entries.is_empty() {
            self.selected = 0;
        } else {
            self.selected = (self.selected + 1).min(self.entries.len() - 1);
        }
    }

    pub fn select_prev(&mut self) {
        if self.entries.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.saturating_sub(1);
        }
    }

    pub fn selected(&self) -> Option<&T> {
        self.entries.get(self.selected)
    }

    pub fn update_entries<F>(&mut self, new_entries: Vec<T>, name_of: F)
    where
        F: Fn(&T) -> &str,
    {
        let prev_name = self.selected().map(|e| name_of(e).to_owned());
        self.entries = new_entries;
        self.restore_selection_by_name(prev_name.as_deref(), name_of);
    }

    pub fn restore_selection_by_name<F>(&mut self, name: Option<&str>, name_of: F)
    where
        F: Fn(&T) -> &str,
    {
        if let Some(name) = name {
            if let Some(pos) = self.entries.iter().position(|e| name_of(e) == name) {
                self.selected = pos;
                return;
            }
        }
        self.clamp_selected();
    }

    pub fn clamp_selected(&mut self) {
        if self.entries.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.entries.len() - 1);
        }
    }
}
