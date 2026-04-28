use crate::{Document, PdfError, PdfResult};
use crate::annotations::AnnRect;
use pdfium_render::prelude::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FormField {
    pub index: u32,
    /// "text" | "checkbox" | "radio" | "combo" | "list" | "push" | "signature" | "other"
    pub kind: String,
    pub name: String,
    /// Current display value (empty for checkbox/radio, use `checked` instead).
    pub value: String,
    /// Options list for combo / list fields.
    pub options: Vec<String>,
    /// True when checkbox or radio button is checked.
    pub checked: bool,
    /// True for multiline text fields.
    pub multiline: bool,
    pub rect: AnnRect,
}

impl Document {
    /// Return "none" | "acro" | "xfa_full" | "xfa_foreground" for this document.
    pub fn form_type(&self) -> PdfResult<String> {
        self.with_doc(|doc| {
            Ok(match doc.form() {
                None => "none".to_string(),
                Some(form) => match form.form_type() {
                    PdfFormType::None => "none",
                    PdfFormType::Acrobat => "acro",
                    PdfFormType::XfaFull => "xfa_full",
                    PdfFormType::XfaForeground => "xfa_foreground",
                }
                .to_string(),
            })
        })
    }

    /// Enumerate all AcroForm fields on the given page.
    pub fn get_form_fields(&self, page_index: u32) -> PdfResult<Vec<FormField>> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            if page_index >= pages.len() as u32 {
                return Err(PdfError::InvalidPage(page_index));
            }
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let pw = page.width().value;
            let ph = page.height().value;

            let mut fields = Vec::new();
            let annots = page.annotations();

            for i in 0..annots.len() {
                let annot = match annots.get(i) {
                    Ok(a) => a,
                    Err(_) => continue,
                };

                let form_field = match annot.as_form_field() {
                    Some(f) => f,
                    None => continue,
                };

                let kind = match form_field.field_type() {
                    PdfFormFieldType::PushButton => "push",
                    PdfFormFieldType::Checkbox => "checkbox",
                    PdfFormFieldType::RadioButton => "radio",
                    PdfFormFieldType::ComboBox => "combo",
                    PdfFormFieldType::ListBox => "list",
                    PdfFormFieldType::Text => "text",
                    PdfFormFieldType::Signature => "signature",
                    PdfFormFieldType::Unknown => "other",
                }
                .to_string();

                let name = form_field.name().unwrap_or_default();

                let (value, checked, multiline, options) = match form_field {
                    PdfFormField::Text(text) => {
                        let v = text.value().unwrap_or_default();
                        let ml = text.is_multiline();
                        (v, false, ml, vec![])
                    }
                    PdfFormField::Checkbox(cb) => {
                        let c = cb.is_checked().unwrap_or(false);
                        (String::new(), c, false, vec![])
                    }
                    PdfFormField::RadioButton(rb) => {
                        let c = rb.is_checked().unwrap_or(false);
                        let v = rb.group_value().unwrap_or_default();
                        (v, c, false, vec![])
                    }
                    PdfFormField::ComboBox(combo) => {
                        let v = combo.value().unwrap_or_default();
                        let opts: Vec<String> = combo
                            .options()
                            .iter()
                            .filter_map(|opt| opt.label().cloned())
                            .collect();
                        (v, false, false, opts)
                    }
                    PdfFormField::ListBox(list) => {
                        let v = list.value().unwrap_or_default();
                        let opts: Vec<String> = list
                            .options()
                            .iter()
                            .filter_map(|opt| opt.label().cloned())
                            .collect();
                        (v, false, false, opts)
                    }
                    _ => (String::new(), false, false, vec![]),
                };

                let rect = annot
                    .bounds()
                    .map(|b| pdf_to_screen(&b, pw, ph))
                    .unwrap_or(AnnRect { left: 0.0, top: 0.0, width: 0.05, height: 0.05 });

                fields.push(FormField {
                    index: i as u32,
                    kind,
                    name,
                    value,
                    checked,
                    multiline,
                    options,
                    rect,
                });
            }
            Ok(fields)
        })
    }

    /// Set the string value of a text field.
    pub fn set_field_text_value(
        &self,
        page_index: u32,
        annot_index: u32,
        value: &str,
    ) -> PdfResult<()> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let mut annot = page
                .annotations()
                .get(annot_index as usize)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            if let Some(field) = annot.as_form_field_mut() {
                if let Some(text) = field.as_text_field_mut() {
                    text.set_value(value)
                        .map_err(|e| PdfError::Render(e.to_string()))?;
                }
            }
            drop(page); // keep page alive until set_value completes
            Ok(())
        })
    }

    /// Reset all text and checkbox fields on a page to empty/unchecked.
    ///
    /// Called when the frontend detects a push-button click (most push buttons
    /// in interactive PDFs are Reset or Submit buttons; we handle the reset case
    /// here and ignore submit since that requires network I/O outside scope).
    pub fn reset_form_fields(&self, page_index: u32) -> PdfResult<()> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            if page_index >= pages.len() as u32 {
                return Err(PdfError::InvalidPage(page_index));
            }
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let annots = page.annotations();
            let count = annots.len();
            for i in 0..count {
                let mut annot = match annots.get(i) {
                    Ok(a) => a,
                    Err(_) => continue,
                };
                if let Some(field) = annot.as_form_field_mut() {
                    match field {
                        PdfFormField::Text(t) => { let _ = t.set_value(""); }
                        PdfFormField::Checkbox(cb) => { let _ = cb.set_checked(false); }
                        _ => {}
                    }
                }
            }
            drop(page);
            Ok(())
        })
    }

    /// Toggle a checkbox field.
    pub fn set_field_checked(
        &self,
        page_index: u32,
        annot_index: u32,
        checked: bool,
    ) -> PdfResult<()> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let mut annot = page
                .annotations()
                .get(annot_index as usize)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            if let Some(field) = annot.as_form_field_mut() {
                if let Some(cb) = field.as_checkbox_field_mut() {
                    cb.set_checked(checked)
                        .map_err(|e| PdfError::Render(e.to_string()))?;
                }
            }
            drop(page);
            Ok(())
        })
    }
}

fn pdf_to_screen(r: &PdfRect, pw: f32, ph: f32) -> AnnRect {
    let left = (r.left().value / pw).clamp(0.0, 1.0);
    let top_s = (1.0 - r.top().value / ph).clamp(0.0, 1.0);
    let w = ((r.right().value - r.left().value) / pw).abs().max(0.001);
    let h = ((r.top().value - r.bottom().value) / ph).abs().max(0.001);
    AnnRect {
        left,
        top: top_s,
        width: w.min(1.0),
        height: h.min(1.0),
    }
}
