pub mod chosen;
pub mod completed;
pub mod deleted;
pub mod dumped;
pub mod failed;
pub mod finished;
pub mod started;
pub mod uploaded;

use chosen::DateChosen;
use completed::Completed;
use deleted::Deleted;
use dumped::Dumped;
use failed::Failed;
use finished::Finished;
use osentities::Id;
use serde::{Deserialize, Serialize};
use started::Started;
use uploaded::Uploaded;

pub trait EventMetadata {
    fn reference(&self) -> Id;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Event {
    /// Archive process started event. Emitted when the archive process is started.
    Started(Started),
    /// Archive process has chosen the date to dump. Emitted when the archive process has chosen the date to dump.
    DateChosen(DateChosen),
    /// Archive process dumped event. Emitted when mongodump finishes dumping the database.
    Dumped(Dumped),
    /// Archive process failed event. Emitted when the archive process fails in some way.
    Failed(Failed),
    /// Archive process uploaded event. Emitted after the selected storage provider uploads any file (by default, the archive file and metadata file).
    Uploaded(Uploaded),
    /// Archive process completed event. Emitted when all dumped files are uploaded.
    Completed(Completed),
    /// Archive process finished event. Emitted when the archive process is finished.
    Finished(Finished),
    /// Archive process deleted event. Emitted when the archive process is deleted.
    Deleted(Deleted),
}

impl Event {
    pub fn is_finished(&self) -> bool {
        matches!(self, Event::Finished(_))
    }
}

impl EventMetadata for Event {
    fn reference(&self) -> Id {
        match self {
            Event::Started(event) => event.reference(),
            Event::DateChosen(event) => event.reference(),
            Event::Dumped(event) => event.reference(),
            Event::Failed(event) => event.reference(),
            Event::Uploaded(event) => event.reference(),
            Event::Completed(event) => event.reference(),
            Event::Finished(event) => event.reference(),
            Event::Deleted(event) => event.reference(),
        }
    }
}
