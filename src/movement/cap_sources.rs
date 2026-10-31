//! The additive `cap_source` trace column: the lifecycle of every exported actor that answers the
//! cap's messages, one `id/...` record per source. Only the validators of
//! `tools/cap_sources.py` read it; no replay input depends on it.
use super::*;

impl Player {
    pub fn cap_source_diagnostic(&self) -> String {
        self.signboards
            .iter()
            .map(sign_board::SignBoard::diagnostic)
            .chain(
                self.cap_hangers
                    .iter()
                    .map(cap_hanger::CapHanger::diagnostic),
            )
            .collect::<Vec<_>>()
            .join("|")
    }
}
