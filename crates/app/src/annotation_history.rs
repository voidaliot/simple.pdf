/// Annotation indexes shift when an earlier annotation on the same page is
/// removed. Track that shift before the next Undo can target an unrelated item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UndoEntry {
    pub page_index: u32,
    pub annot_index: u32,
}

#[derive(Default)]
pub struct AnnotationHistory {
    entries: Vec<UndoEntry>,
}

impl AnnotationHistory {
    pub fn record(&mut self, page_index: u32, annot_index: u32) {
        self.entries.push(UndoEntry {
            page_index,
            annot_index,
        });
        if self.entries.len() > 100 {
            self.entries.remove(0);
        }
    }

    pub fn last(&self) -> Option<UndoEntry> {
        self.entries.last().cloned()
    }

    pub fn removed(&mut self, page_index: u32, annot_index: u32) {
        self.entries
            .retain(|entry| entry.page_index != page_index || entry.annot_index != annot_index);
        for entry in &mut self.entries {
            if entry.page_index == page_index && entry.annot_index > annot_index {
                entry.annot_index -= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleting_an_earlier_annotation_retargets_undo() {
        let mut history = AnnotationHistory::default();
        history.record(0, 4);
        history.record(1, 7);
        history.record(0, 8);
        history.removed(0, 2);
        assert_eq!(
            history.last(),
            Some(UndoEntry {
                page_index: 0,
                annot_index: 7
            })
        );
        history.removed(0, 7);
        assert_eq!(
            history.last(),
            Some(UndoEntry {
                page_index: 1,
                annot_index: 7
            })
        );
        history.removed(1, 7);
        assert_eq!(
            history.last(),
            Some(UndoEntry {
                page_index: 0,
                annot_index: 3
            })
        );
    }

    #[test]
    fn deleting_the_last_added_annotation_does_not_leave_a_stale_undo() {
        let mut history = AnnotationHistory::default();
        history.record(0, 0);
        history.removed(0, 0);
        assert_eq!(history.last(), None);
    }

    #[test]
    fn history_memory_is_bounded() {
        let mut history = AnnotationHistory::default();
        for index in 0..150 {
            history.record(0, index);
        }
        assert_eq!(history.entries.len(), 100);
        assert_eq!(history.entries[0].annot_index, 50);
    }
}
