use gpui::SharedString;

use crate::text::{
    document::ParsedDocument,
    node::{BlockNode, NodeContext, Paragraph, Span},
};

/// Parse plain text into a single paragraph node, verbatim.
///
/// Unlike the markdown and HTML formats this does not interpret the source at
/// all: no emphasis, no headings, no block structure. Newlines and leading
/// whitespace survive as written — the paragraph's text is shaped by gpui,
/// which breaks lines on `\n` — so literal content (a pasted snippet,
/// pretty-printed JSON) reads exactly as it was written while still getting
/// [`super::super::TextView`]'s selection and copy.
pub(crate) fn parse(source: &str, cx: &mut NodeContext) -> ParsedDocument {
    let mut paragraph = Paragraph::default();
    paragraph.push_str(source);
    paragraph.set_span(Span {
        start: cx.offset,
        end: cx.offset + source.len(),
    });

    ParsedDocument {
        source: SharedString::from(source.to_owned()),
        blocks: vec![BlockNode::Paragraph(paragraph)],
    }
}
