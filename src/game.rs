use { asr::Process, async_trait::async_trait };

use crate::AutoSplitter;

pub mod risk_of_rain;
pub mod risk_of_rain_2;
pub mod risk_of_rain_returns;

#[async_trait]
pub trait GameAutoSplitter {

    // Autosplitter utility

    /// Process name(s) the game can attach to
    fn process_names(&self) -> &[&str];

    /// Expose game settings
    ///
    /// NOTE: `new()` may already register settings automatically
    fn register_settings(&mut self);

    /// Helper that attaches to any of the process names
    fn attach_any(&self) -> Option<Process> {
        for process_name in self.process_names() {
            let process = Process::attach(platform_process_name(process_name));

            if process.is_some() {
                return process;
            }
        }
        return None;
    }


    // Game "main()" equivalent

    /// Hooks to the game process and manages internal game autosplitter state
    ///
    /// This launches the main autosplitter update loop when hooked
    async fn attached(&mut self, process: &Process, autosplitter: &mut AutoSplitter);


    // Splitting logic

    /// Returns true if game is in a starting condition
    fn start(&self) -> bool;

    /// Returns true if game has reached the start condition
    fn reset(&self) -> bool;

    /// Returns true if game met a split condition
    ///
    /// This explicitly ignores the game end condition
    fn split(&self) -> bool;

    /// Returns true if game met the completion condition
    ///
    /// This is used by the autosplitter to know when games need to be swapped
    fn completed(&self) -> bool;

    /// Returns true if load times need to be removed
    ///
    /// None indicates undetermined loading state, which behaves by maintaining the previously known state
    fn is_loading(&self) -> Option<bool>;
}

/// Cross-platform process name
///
/// On Windows: full exe name
/// On Linux: exe name truncated to 15 characters
fn platform_process_name(process_name: &str) -> &str {
    if asr::get_os().ok().unwrap().starts_with("linux") {
        return &process_name[0..15];
    } else {
        return process_name;
    };
}
