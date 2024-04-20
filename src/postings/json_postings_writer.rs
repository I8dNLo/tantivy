use std::io;

use common::json_path_writer::JSON_END_OF_PATH;
use stacker::Addr;

use crate::indexer::doc_id_mapping::DocIdMapping;
use crate::indexer::path_to_unordered_id::OrderedPathId;
use crate::postings::postings_writer::SpecializedPostingsWriter;
use crate::postings::recorder::{BufferLender, DocIdRecorder, Recorder};
use crate::postings::{FieldSerializer, IndexingContext, IndexingPosition, PostingsWriter};
use crate::schema::indexing_term::IndexingTerm;
use crate::schema::{Field, Type, ValueBytes};
use crate::tokenizer::TokenStream;
use crate::DocId;

/// The `JsonPostingsWriter` is odd in that it relies on a hidden contract:
///
/// `subscribe` is called directly to index non-text tokens, while
/// `index_text` is used to index text.
#[derive(Default)]
pub(crate) struct JsonPostingsWriter<Rec: Recorder> {
    str_posting_writer: SpecializedPostingsWriter<Rec>,
    non_str_posting_writer: SpecializedPostingsWriter<DocIdRecorder>,
}

impl<Rec: Recorder> From<JsonPostingsWriter<Rec>> for Box<dyn PostingsWriter> {
    fn from(json_postings_writer: JsonPostingsWriter<Rec>) -> Box<dyn PostingsWriter> {
        Box::new(json_postings_writer)
    }
}

impl<Rec: Recorder> PostingsWriter for JsonPostingsWriter<Rec> {
    #[inline]
    fn subscribe(
        &mut self,
        doc: crate::DocId,
        pos: u32,
        term: &IndexingTerm,
        ctx: &mut IndexingContext,
    ) {
        self.non_str_posting_writer.subscribe(doc, pos, term, ctx);
    }

    fn index_text(
        &mut self,
        doc_id: DocId,
        token_stream: &mut dyn TokenStream,
        term_buffer: &mut IndexingTerm,
        ctx: &mut IndexingContext,
        indexing_position: &mut IndexingPosition,
    ) {
        self.str_posting_writer.index_text(
            doc_id,
            token_stream,
            term_buffer,
            ctx,
            indexing_position,
        );
    }

    /// The actual serialization format is handled by the `PostingsSerializer`.
    fn serialize(
        &self,
        term_addrs: &[(Field, OrderedPathId, &[u8], Addr)],
        ordered_id_to_path: &[&str],
        doc_id_map: Option<&DocIdMapping>,
        ctx: &IndexingContext,
        serializer: &mut FieldSerializer,
    ) -> io::Result<()> {
        let mut term_buffer = JsonTermSerializer(Vec::with_capacity(48));
        let mut buffer_lender = BufferLender::default();
        let mut prev_term_id = u32::MAX;
        let mut term_path_len = 0; // this will be set in the first iteration
        for (_field, path_id, term, addr) in term_addrs {
            if prev_term_id != path_id.path_id() {
                term_buffer.clear();
                term_buffer.append_path(ordered_id_to_path[path_id.path_id() as usize].as_bytes());
                term_buffer.append_bytes(&[JSON_END_OF_PATH]);
                term_path_len = term_buffer.len();
                prev_term_id = path_id.path_id();
            }
            term_buffer.truncate(term_path_len);
            term_buffer.append_bytes(term);
            let json_value = ValueBytes::wrap(term);
            let typ = json_value.typ();
            if typ == Type::Str {
                SpecializedPostingsWriter::<Rec>::serialize_one_term(
                    term_buffer.as_bytes(),
                    *addr,
                    doc_id_map,
                    &mut buffer_lender,
                    ctx,
                    serializer,
                )?;
            } else {
                SpecializedPostingsWriter::<DocIdRecorder>::serialize_one_term(
                    term_buffer.as_bytes(),
                    *addr,
                    doc_id_map,
                    &mut buffer_lender,
                    ctx,
                    serializer,
                )?;
            }
        }
        Ok(())
    }

    fn total_num_tokens(&self) -> u64 {
        self.str_posting_writer.total_num_tokens() + self.non_str_posting_writer.total_num_tokens()
    }
}

struct JsonTermSerializer(Vec<u8>);
impl JsonTermSerializer {
    #[inline]
    pub fn append_path(&mut self, bytes: &[u8]) {
        if bytes.contains(&0u8) {
            self.0
                .extend(bytes.iter().map(|&b| if b == 0 { b'0' } else { b }));
        } else {
            self.0.extend_from_slice(bytes);
        }
    }

    /// Appends value bytes to the Term.
    ///
    /// This function returns the segment that has just been added.
    #[inline]
    pub fn append_bytes(&mut self, bytes: &[u8]) -> &mut [u8] {
        let len_before = self.0.len();
        self.0.extend_from_slice(bytes);
        &mut self.0[len_before..]
    }

    fn clear(&mut self) {
        self.0.clear();
    }
    fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }
    fn len(&self) -> usize {
        self.0.len()
    }

    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
