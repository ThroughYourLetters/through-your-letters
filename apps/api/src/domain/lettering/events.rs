use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LetteringEvent {
    Uploaded {
        lettering_id: Uuid,
    },
    Approved {
        lettering_id: Uuid,
    },
    Rejected {
        lettering_id: Uuid,
    },
    Liked {
        lettering_id: Uuid,
    },
    Commented {
        lettering_id: Uuid,
        comment_id: Uuid,
    },
}
