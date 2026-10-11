use rcad_doc::Project;

const MAX_STACK: usize = 64;

#[derive(Default)]
pub struct UndoStack {
    undo: Vec<Project>,
    redo: Vec<Project>,
}

impl UndoStack {
    pub fn push(&mut self, before: Project) {
        self.undo.push(before);
        if self.undo.len() > MAX_STACK {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: &mut Project) -> bool {
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo.push(current.clone());
        *current = prev;
        true
    }

    pub fn redo(&mut self, current: &mut Project) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(current.clone());
        *current = next;
        true
    }
}
