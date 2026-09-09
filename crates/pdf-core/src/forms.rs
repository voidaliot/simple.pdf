use crate::annotations::AnnRect;
use crate::{Document, PdfError, PdfResult};
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
    /// Includes field types whose values the pinned PDFium wrapper cannot write.
    pub read_only: bool,
    pub rect: AnnRect,
    /// For push buttons: "reset" | "submit" | "other". Always "none" for non-button fields.
    /// Detected via field-name heuristic (PDF action dict requires raw FFI, deferred).
    pub action_type: String,
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
                let read_only = form_field.is_read_only()
                    || matches!(kind.as_str(), "combo" | "list" | "signature" | "other");

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

                let action_type = if kind == "push" {
                    classify_button_action(&name).to_string()
                } else {
                    "none".to_string()
                };

                let rect = annot
                    .bounds()
                    .map(|b| pdf_to_screen(&b, pw, ph))
                    .unwrap_or(AnnRect {
                        left: 0.0,
                        top: 0.0,
                        width: 0.05,
                        height: 0.05,
                    });

                fields.push(FormField {
                    index: i as u32,
                    kind,
                    name,
                    value,
                    checked,
                    multiline,
                    read_only,
                    options,
                    rect,
                    action_type,
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
            if page_index >= pages.len() as u32 {
                return Err(PdfError::InvalidPage(page_index));
            }
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let mut annot = page
                .annotations()
                .get(annot_index as usize)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let field = annot
                .as_form_field_mut()
                .ok_or_else(|| PdfError::Render("Annotation is not a form field".into()))?;
            if field.is_read_only() {
                return Err(PdfError::Render("This field is read-only".into()));
            }
            let text = field.as_text_field_mut().ok_or_else(|| {
                PdfError::Render("Editing this field type is not supported".into())
            })?;
            text.set_value(value)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            drop(page); // keep page alive until set_value completes
            Ok(())
        })
    }

    /// Reset all text and checkbox fields on a single page.
    pub fn reset_form_fields(&self, page_index: u32) -> PdfResult<()> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            if page_index >= pages.len() as u32 {
                return Err(PdfError::InvalidPage(page_index));
            }
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            reset_page_fields(&page)?;
            drop(page);
            Ok(())
        })
    }

    /// Reset all text and checkbox fields across every page in the document.
    ///
    /// Clears writable text and checkbox values. Restoring arbitrary PDF default
    /// values and action field lists is not supported by this wrapper.
    pub fn reset_all_form_fields(&self) -> PdfResult<()> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let page_count = pages.len();
            for pi in 0..page_count {
                let page = pages.get(pi).map_err(|e| PdfError::Render(e.to_string()))?;
                reset_page_fields(&page)?;
                drop(page);
            }
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
            if page_index >= pages.len() as u32 {
                return Err(PdfError::InvalidPage(page_index));
            }
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let mut annot = page
                .annotations()
                .get(annot_index as usize)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let field = annot
                .as_form_field_mut()
                .ok_or_else(|| PdfError::Render("Annotation is not a form field".into()))?;
            if field.is_read_only() {
                return Err(PdfError::Render("This field is read-only".into()));
            }
            match field {
                PdfFormField::Checkbox(cb) => cb.set_checked(checked),
                PdfFormField::RadioButton(radio) if checked => radio.set_checked(),
                _ => return Err(PdfError::Render("This field cannot be toggled".into())),
            }
            .map_err(|e| PdfError::Render(e.to_string()))?;
            drop(page);
            Ok(())
        })
    }
}

fn reset_page_fields(page: &pdfium_render::prelude::PdfPage<'_>) -> PdfResult<()> {
    let annots = page.annotations();
    let count = annots.len();
    for i in 0..count {
        let mut annot = annots.get(i).map_err(|e| PdfError::Render(e.to_string()))?;
        if let Some(field) = annot.as_form_field_mut() {
            if field.is_read_only() {
                continue;
            }
            match field {
                PdfFormField::Text(t) => {
                    t.set_value("")
                        .map_err(|e| PdfError::Render(e.to_string()))?;
                }
                PdfFormField::Checkbox(cb) => {
                    cb.set_checked(false)
                        .map_err(|e| PdfError::Render(e.to_string()))?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Classify a push button's likely action from its field name.
///
/// PDF action dicts require raw FFI to inspect; this heuristic covers the
/// common case where the designer used a descriptive field name. An ambiguous
/// name must never clear the user's form values.
fn classify_button_action(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    let is_submit = ["submit", "send", "envoyer", "einreichen", "enviar", "invia"]
        .iter()
        .any(|kw| lower.contains(kw));
    let is_print = ["print", "drucken", "imprimer", "imprimir"]
        .iter()
        .any(|kw| lower.contains(kw));
    let is_reset = ["reset", "clear", "zurücksetzen", "effacer", "réinitialiser"]
        .iter()
        .any(|kw| lower.contains(kw));
    if is_submit {
        "submit"
    } else if is_print {
        "print"
    } else if is_reset {
        "reset"
    } else {
        "other"
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

#[cfg(test)]
mod tests {
    use super::classify_button_action;
    use crate::{PdfEngine, PdfError, DOCUMENT_TEST_GATE};

    #[test]
    fn ambiguous_push_buttons_never_clear_form_values() {
        for name in ["", "Button1", "Calculate", "Next page", "Print", "Send"] {
            assert_ne!(classify_button_action(name), "reset");
        }
        assert_eq!(classify_button_action("Reset form"), "reset");
        assert_eq!(classify_button_action("Clear fields"), "reset");
    }

    #[test]
    fn form_edits_persist_and_read_only_or_unsupported_edits_fail() {
        let _serial = DOCUMENT_TEST_GATE.lock();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("fields.pdf");
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Annots [6 0 R 7 0 R 8 0 R 9 0 R 11 0 R] >>",
            "<< /Fields [6 0 R 7 0 R 8 0 R 9 0 R 10 0 R] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (text) /V (before) /Rect [10 10 100 30] /P 3 0 R >>",
            "<< /Type /Annot /Subtype /Widget /FT /Tx /Ff 1 /T (readonly) /V (fixed) /Rect [10 40 100 60] /P 3 0 R >>",
            "<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 131072 /T (choice) /Opt [(One) (Two)] /V (One) /Rect [10 70 100 90] /P 3 0 R >>",
            "<< /Type /Annot /Subtype /Widget /FT /Btn /T (checkbox) /V /Off /AS /Off /AP << /N << /Yes 12 0 R /Off 13 0 R >> >> /Rect [10 100 30 120] /P 3 0 R >>",
            "<< /FT /Btn /Ff 32768 /T (radio) /Kids [11 0 R] /V /Off >>",
            "<< /Type /Annot /Subtype /Widget /Parent 10 0 R /AS /Off /AP << /N << /Selected 12 0 R /Off 13 0 R >> >> /Rect [10 130 30 150] /P 3 0 R >>",
            "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length 0 >>\nstream\n\nendstream",
            "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length 0 >>\nstream\n\nendstream",
        ];
        let mut pdf = String::from("%PDF-1.7\n");
        let mut offsets = vec![0];
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{}\nendobj\n", index + 1, object));
        }
        let xref = pdf.len();
        pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()));
        for offset in offsets.iter().skip(1) {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        ));
        std::fs::write(&path, pdf).unwrap();
        let engine = PdfEngine::new(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/pdfium"),
        )
        .unwrap();
        let document = engine.open(&path).unwrap();
        let fields = document.get_form_fields(0).unwrap();
        assert_eq!(fields.len(), 5);
        assert!(fields[1].read_only && fields[2].read_only);
        assert!(document.set_field_text_value(0, 1, "overwrite").is_err());
        assert!(document.set_field_text_value(0, 2, "Two").is_err());
        assert!(matches!(
            document.set_field_text_value(65_536, 0, "wrong page"),
            Err(PdfError::InvalidPage(65_536))
        ));
        document.set_field_text_value(0, 0, "after").unwrap();
        document.set_field_checked(0, 3, true).unwrap();
        document.set_field_checked(0, 4, true).unwrap();
        document.save_to_path(&path).unwrap();
        drop(document);
        let document = engine.open(&path).unwrap();
        let fields = document.get_form_fields(0).unwrap();
        assert_eq!(fields[0].value, "after");
        assert_eq!(fields[1].value, "fixed");
        assert!(fields[3].checked && fields[4].checked);
        document.reset_all_form_fields().unwrap();
        let fields = document.get_form_fields(0).unwrap();
        assert_eq!(fields[0].value, "");
        assert_eq!(fields[1].value, "fixed");
    }
}
