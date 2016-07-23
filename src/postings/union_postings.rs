
use DocId;
use postings::{Postings, DocSet};
use std::collections::BinaryHeap;
use postings::SkipResult;
use std::cmp::Ordering;
use query::MultiTermScorer;
use postings::ScoredDocSet;
use query::Scorer;

#[derive(Eq, PartialEq)]
struct HeapItem(DocId, usize, u32);

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other:&Self) -> Option<Ordering> {
         (self.0, self.1).partial_cmp(&(other.0, other.1)).map(|o| o.reverse())
    }
}

impl Ord for HeapItem {
    fn cmp(&self, other:&Self) -> Ordering {
         (self.0, self.1).cmp(&(other.0, other.1)).reverse()
    }
}

pub struct UnionPostings<TPostings: Postings> {
    postings: Vec<TPostings>,
    queue: BinaryHeap<HeapItem>,
    doc: DocId,
    scorer: MultiTermScorer
}

impl<TPostings: Postings> UnionPostings<TPostings> {
    pub fn new(postings: Vec<TPostings>, multi_term_scorer: MultiTermScorer) -> UnionPostings<TPostings> {
        let num_postings = postings.len();
        let mut union_postings = UnionPostings {
            postings: postings,
            queue: BinaryHeap::new(),
            doc: 0,
            scorer: multi_term_scorer
        };
        for ord in 0..num_postings {
            union_postings.enqueue(ord);    
        }
        union_postings
    }

    fn enqueue(&mut self, ord: usize) {
        let cur_postings = &mut self.postings[ord];
        if cur_postings.next() {
            let doc = cur_postings.doc();
            let tf = cur_postings.term_freq();
            self.queue.push(HeapItem(doc, ord, tf));
        }
    }

}

impl<TPostings: Postings> DocSet for UnionPostings<TPostings> {
    
    fn next(&mut self,) -> bool {
        self.scorer.clear();

        // if self.active_posting_ordinals.is_empty() {
        //     return false;
        // }

        // for &ord in self.active_posting_ordinals.iter() {
        //     let cur_postings = &self.postings[ord];
        //     if cur_postings.next() {
        //         let doc = cur_postings.doc();
        //         let tf = cur_postings.freq();
        //         self.queue.push(HeapItem(doc, ord, tf));
        //     }
        // }
        // self.active_posting_ordinals.clear();
        
        let head = self.queue.pop(); 
        match head {
            Some(HeapItem(doc, ord, tf)) => {
                self.scorer.update(ord, tf);
                self.enqueue(ord);
                self.doc = doc;
                loop {
                    {
                        let peek = self.queue.peek();
                        match peek {
                            Some(&HeapItem(peek_doc, _, _))  => {
                                if peek_doc != doc {
                                    break;
                                }
                            }
                            None => { break; }   
                        }
                    }
                    let HeapItem(_, peek_ord, peek_tf) = self.queue.pop().unwrap();
                    self.scorer.update(peek_ord, peek_tf);
                    self.enqueue(peek_ord);
                }
            }
            None => {
                return false;
            }
        }
        return true;
    }


    fn skip_next(&mut self, _: DocId) -> SkipResult {
        SkipResult::End
    }
    
    fn doc(&self,) -> DocId {
        self.doc
    }
        
    fn doc_freq(&self,) -> usize {
        panic!("Doc freq");
    }
}

impl<TPostings: Postings> ScoredDocSet for UnionPostings<TPostings> {
    fn score(&self,) -> f32 {
        self.scorer.score()
    }
}

#[cfg(test)]
mod tests {
    
    use super::*;
    use postings::{DocSet, VecPostings};
    use query::MultiTermScorer;

    #[test]
    pub fn test_union_postings() {
        let left = VecPostings::new(vec!(1, 2, 3));
        let right = VecPostings::new(vec!(1, 3, 8));
        let multi_term_scorer = MultiTermScorer::new(vec!(1f32, 2f32), vec!(1f32, 4f32));
        let mut union = UnionPostings::new(vec!(left, right), multi_term_scorer);
        assert!(union.next());
        assert_eq!(union.doc(), 1);
        // assert_eq!(union.active_posting_ordinals(), [0, 1]);
        assert!(union.next());
        assert_eq!(union.doc(), 2);
        // assert_eq!(union.active_posting_ordinals(), [0]);
        assert!(union.next());
        assert_eq!(union.doc(), 3);
        // assert_eq!(union.active_posting_ordinals(), [0, 1]);
        assert!(union.next());
        assert_eq!(union.doc(), 8);
        // assert_eq!(union.active_posting_ordinals(), [1]);
        assert!(!union.next());
    }

}

