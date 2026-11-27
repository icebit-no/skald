//! What the window asks of the app. The app does the work and hands the window its model again.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Fetch the model, go on from where its download stopped, or check a file already there. Also the
    /// retry after an error.
    StartDownload,
    /// Stop the download and keep what arrived.
    CancelDownload,
    /// Ask the system for the permission, which shows its own prompt.
    RequestAccess(Permission),
    /// Open the permission's pane in the system's settings, where a refusal is undone.
    OpenSettings(Permission),
    /// The window came to the front again, as when the user comes back from the system's settings:
    /// read the permissions afresh.
    CheckPermissions,
    /// Load the model and listen for the hold key; each take's result goes into the model's test.
    RunTest,
    /// The user is done; the window has closed itself.
    Done,
    /// The user closed the window before they were done.
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Microphone,
    /// macOS only: the key listener and the paste both need it.
    Accessibility,
}
