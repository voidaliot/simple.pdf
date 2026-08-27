use crate::{Document, PdfResult};
use pdfium_render::prelude::*;
use std::collections::HashSet;

/// A target activated by a PDF link annotation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LinkTarget {
    Page { page_index: u32 },
    Uri { uri: String },
}

/// One document-outline entry in depth-first display order.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct OutlineItem {
    pub title: String,
    pub page_index: Option<u32>,
    pub depth: u32,
}

impl Document {
    pub fn document_outline(&self) -> PdfResult<Vec<OutlineItem>> {
        self.with_doc(|doc| {
            const MAX_OUTLINE_ITEMS: usize = 10_000;
            const MAX_OUTLINE_DEPTH: u32 = 128;

            let mut result = Vec::new();
            let mut visited = HashSet::new();
            let mut pending = Vec::new();
            if let Some(root) = doc.bookmarks().root() {
                pending.push((root, 0));
            }

            // Explicit traversal is required here: pdfium-render's public
            // `parent()` value does not retain its own parent, so deriving a
            // node's full depth after the fact would collapse depth > 1.
            // Push the sibling first so the child is popped/visited first.
            while let Some((bookmark, depth)) = pending.pop() {
                if result.len() >= MAX_OUTLINE_ITEMS || !visited.insert(bookmark.clone()) {
                    continue;
                }
                if let Some(sibling) = bookmark.next_sibling() {
                    pending.push((sibling, depth));
                }
                if depth < MAX_OUTLINE_DEPTH {
                    if let Some(child) = bookmark.first_child() {
                        pending.push((child, depth + 1));
                    }
                }
                result.push(OutlineItem {
                    title: bookmark.title().unwrap_or_default(),
                    page_index: bookmark_page_index(&bookmark),
                    depth,
                });
            }
            Ok(result)
        })
    }
}

/// Resolves a page link using its direct destination first, followed by its
/// action. PDF producers commonly use either representation.
pub(crate) fn link_target(link: &PdfLink<'_>) -> Option<LinkTarget> {
    if let Some(page_index) = link
        .destination()
        .and_then(|destination| destination.page_index().ok())
    {
        return Some(LinkTarget::Page {
            page_index: u32::from(page_index),
        });
    }

    match link.action()? {
        PdfAction::LocalDestination(action) => action
            .destination()
            .ok()
            .and_then(|destination| destination.page_index().ok())
            .map(|page_index| LinkTarget::Page {
                page_index: u32::from(page_index),
            }),
        PdfAction::Uri(action) => action.uri().ok().and_then(|uri| {
            let uri = uri.trim();
            (!uri.is_empty()).then(|| LinkTarget::Uri { uri: uri.into() })
        }),
        _ => None,
    }
}

fn bookmark_page_index(bookmark: &PdfBookmark<'_>) -> Option<u32> {
    if let Some(page_index) = bookmark
        .destination()
        .and_then(|destination| destination.page_index().ok())
    {
        return Some(u32::from(page_index));
    }

    match bookmark.action()? {
        PdfAction::LocalDestination(action) => action
            .destination()
            .ok()
            .and_then(|destination| destination.page_index().ok())
            .map(u32::from),
        _ => None,
    }
}
