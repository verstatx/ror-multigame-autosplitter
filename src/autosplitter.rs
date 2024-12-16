use asr::{settings::{Gui, gui::Title}, time::Duration, timer, timer::TimerState};

use crate::game::GameAutoSplitter;

#[derive(Gui)]
pub struct AutoSplitterSettings {
    /// General Settings
    pub _general_settings: Title,
    /// Allow the autosplitter to start the timer automatically
    #[default = true]
    pub start: bool,
    /// Allow the autosplitter to split automatically
    ///
    /// See individual game settings for more control over splits
    #[default = true]
    pub split: bool,
    /// Allow the autosplitter to reset automatically
    ///
    /// Automatic resets are disabled after the first split even if splitting is disabled
    #[default = true]
    pub reset: bool,
}

/// Timer state for update loop
#[derive(Default)]
pub struct AutoSplitterState {
    /// For tracking timer pause between games
    pub switching_games: bool,
    /// Avoids unwanted resets
    pub autoreset_lockout: bool,
    /// Prevents flodding the runtime with pause/resume commands
    pub was_loading: bool,
}

pub struct AutoSplitter {
    settings: AutoSplitterSettings,
    state: AutoSplitterState,
    //game_splitter: Option<&dyn GameAutoSplitter>, // ERROR something something not Send
}

impl AutoSplitter {
    pub fn new() -> Self { Self { settings: AutoSplitterSettings::register(), state: AutoSplitterState::default() } }

    fn reset_state(&mut self) {
        self.state = AutoSplitterState::default();
    }

    /// FIXME Dirty hack results in game time being marginally shorter than real time (<1ms)
    fn initialize_game_time_workaround() {
        timer::set_game_time(Duration::ZERO);
    }

    /// Splitting logic update loop runs once per tick
    pub(crate) fn update_loop(&mut self, game_splitter: Option<&dyn GameAutoSplitter>) {
        self.settings.update();

        // Disconnected from all games
        if game_splitter.is_none() {
            match timer::state() {
                TimerState::Running | TimerState::Paused => {
                    if self.state.switching_games && !self.state.was_loading {
                        timer::pause_game_time();
                        self.state.was_loading = true;
                    }
                },

                TimerState::Ended => { self.reset_state(); },

                _ => ()
            }
        }

        // Connected to any game
        let Some(game_splitter) = game_splitter else { return; };

        match timer::state() {
            TimerState::NotRunning => {
                if Self::should_start(game_splitter) {
                    if self.settings.start {
                        timer::start();
                        Self::initialize_game_time_workaround(); // FIXME remove when supported upstream
                    }
                    self.reset_state();
                }
            },

            TimerState::Running | TimerState::Paused => {
                // Reset logic
                if self.should_reset(game_splitter) && self.settings.reset {
                    timer::reset();
                    self.reset_state();
                }
                // Splitting logic
                if !self.state.switching_games {
                    if Self::game_completed(game_splitter) {
                        timer::split();
                        self.state.autoreset_lockout = true; // Disable autoresets in case stage splits are disabled
                        self.state.switching_games = true; // pause timer until game swap is completed
                    } else if Self::should_split(game_splitter) {
                        if self.settings.split {
                            timer::split();
                        }
                        self.state.autoreset_lockout = true; // Disable autoresets after the first split
                    }
                }
                // Resume timer after game swap
                if self.state.switching_games && Self::should_start(game_splitter) {
                    self.state.switching_games = false;
                }
                // Load removal/timer pause for game swap
                if self.is_loading(game_splitter) {
                    if !self.state.was_loading {
                        timer::pause_game_time();
                        self.state.was_loading = true;
                    }
                } else {
                    if self.state.was_loading {
                        timer::resume_game_time();
                        self.state.was_loading = false;
                    }
                }
            },

            TimerState::Ended => { self.reset_state(); },
            TimerState::Unknown => (),

            _ => todo!("New timer states have been added. The autosplitter needs to be updated.")
        }
    }

    fn should_start(game_splitter: &dyn GameAutoSplitter) -> bool {
        return game_splitter.start();
    }

    fn should_reset(&self, game_splitter: &dyn GameAutoSplitter) -> bool {
        return !self.state.autoreset_lockout && game_splitter.reset();
    }

    fn should_split(game_splitter: &dyn GameAutoSplitter) -> bool {
        return game_splitter.split();
    }

    fn game_completed(game_splitter: &dyn GameAutoSplitter) -> bool {
        return game_splitter.completed();
    }

    fn is_loading(&self, game_splitter: &dyn GameAutoSplitter) -> bool {
        return self.state.switching_games || game_splitter.is_loading().unwrap_or(self.state.was_loading);
    }
}
