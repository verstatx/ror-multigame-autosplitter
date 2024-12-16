use asr::{deep_pointer::DeepPointer, future::{next_tick, retry}, Process, settings::{Gui, gui::Title}, watcher::Watcher};
use async_trait::async_trait;
use derive;

#[cfg(debug_output)]
use asr::timer;

use crate::game;
use crate::AutoSplitter;

const TARGET_PROCESS_NAMES : [&str; 2] = ["ROR_GMS_controller.exe", "Risk of Rain.exe"];

#[derive(Gui)]
pub struct GameSettings {
    /// Risk of Rain Settings
    pub _ror1_settings: Title,
    /// Split on stage transitions
    #[default = false]
    pub ror1_stages: bool,
}

/// Game state watchers
#[derive(Default)]
pub struct GameVars {
    /// GameMaker room ID
    pub room: Watcher<i32>,
    /// Control Panel activated after the Providence fight
    ///
    /// This variable is only active on the final stage
    pub run_end_flag: Watcher<i32>,
    /// Time Alive
    pub in_game_time: Watcher<f64>,
}

/// Only supports v1.2.2
pub struct Game {
    pub settings: GameSettings,
    pub game_state: GameVars,
}

impl Game {
    pub fn new() -> Self { Self { settings: GameSettings::register(), game_state: GameVars::default() } }

    fn reset_state(&mut self) {
        self.game_state = GameVars::default();
    }
}


// room ids:
// 0 => white screen pre-intro
// 1 => Hopoo games logo screen
// 9, 10, 11, 12, 13, 14 => intro cutscene
// 2 => main menu
// 3 => Item Log
// 4 => Monster Log
// 5 => Scores and Unlockables
// 39 => Start online co-op menu
//
// 6 => Single player lobby
// 7 => Local co-op lobby
// 40 => Online co-op lobby
//
// 18-38 => Stages & variants
// 41 => Contact Light
//
// 16 => Outro cutscene pt1: cinematic
// 17 => Outro cutscene pt2: character ending
// 15 => Credits

const MENU_ROOMS : [i32; 16] = [0, 1, 2, 3, 4, 5, 9, 10, 11, 12, 13, 14, 15, 16, 17, 39];
const LOBBY_ROOMS : [i32; 3] = [6, 7, 40];

#[async_trait]
impl game::GameAutoSplitter for Game {
    /// "ROR_GMS_controller.exe" or "Risk of Rain.exe"
    fn process_names(&self) -> &[&str] { return &TARGET_PROCESS_NAMES; }

    fn register_settings(&mut self) {
        self.settings = GameSettings::register();
    }

    async fn attached(&mut self, process: &Process, autosplitter: &mut AutoSplitter) {
        self.reset_state();

        // ugly way to get the main module address; LSO provides no way to get the currently attached process name
        let (main_module, _main_module_size) = retry(|| {
            TARGET_PROCESS_NAMES.iter().find_map(|&m| process.get_module_range(m).ok() )
        }).await;

        // Log main module size (differs on Linux)
        #[cfg(debug_output)] print_message(&_main_module_size.to_string());

        let room = DeepPointer::<1>::new_32bit(main_module, &[0x2BED7A8]);
        let run_end_flag = DeepPointer::<5>::new_32bit(main_module, &[0x2BEB5E0, 0x0, 0x548, 0xC, 0xB4]);
        let in_game_time = DeepPointer::<10>::new_32bit(main_module, &[0x02BEB5E0, 0x0, 0x28, 0xC, 0xBC, 0x8, 0x0, 0x720, 0x8, 0x1EC0]);

        loop {
            // update game state watchers
            self.game_state.room.update(
                match room.deref::<i32>(&process) {
                    Ok(val) => Some(val),
                    _ => None
                }
            );
            self.game_state.run_end_flag.update(
                match run_end_flag.deref::<i32>(&process) {
                    Ok(val) => Some(val),
                    _ => None
                }
            );
            self.game_state.in_game_time.update(
                match in_game_time.deref::<f64>(&process) {
                    Ok(val) => Some(val),
                    _ => None
                }
            );

            // show game state for debugging
            #[cfg(debug_output)] {
                match self.game_state.room.pair {
                    Some(room) => timer::set_variable("[RoR1] room ID", &format!("{0:?}", room.current)),
                    _ => timer::set_variable("[RoR1] room ID", "[invalid]")
                }
                match self.game_state.run_end_flag.pair {
                    Some(run_end_flag) => timer::set_variable("[RoR1] run end flag", &format!("{0:?}", run_end_flag.current)),
                    _ => timer::set_variable("[RoR1] run end flag", "[invalid]")
                }
                match self.game_state.in_game_time.pair {
                    Some(in_game_time) => timer::set_variable("[RoR1] In-Game Time", &format!("{0:?}", in_game_time.current)),
                    _ => timer::set_variable("[RoR1] In-Game Time", "[invalid]")
                }
            }

            // Log room ID changes
            #[cfg(debug_output)]
            if let Some(room) = self.game_state.room.pair {
                if room.changed() {
                    asr::print_message(&format!("{0:?}", room.current))
                }
            }

            self.settings.update();
            // cede control to main autosplitter logic loop
            autosplitter.update_loop(Some(self));
            next_tick().await;
        }
    }

    /// Start when entering a game from a lobby
    ///
    /// Simply checks that the room ID went from a lobby to a non-menu/cutscene/lobby room
    fn start(&self) -> bool {
        if let Some(room) = self.game_state.room.pair {
            return room.changed() && LOBBY_ROOMS.contains(&room.old) && !MENU_ROOMS.contains(&room.current);
        }
        return false;
    }

    /// Reset when entering the main menu or lobby
    ///
    /// Specifically detect room IDs 2 (rStart) and 40 (rSelectMult)
    fn reset(&self) -> bool {
        if let Some(room) = self.game_state.room.pair {
            return room.current == 2 || room.current == 40;
        }
        return false;
    }

    /// Split on stage change
    fn split(&self) -> bool {

        // Stage/room changed
        if let Some(room) = self.game_state.room.pair {
            if room.changed() {
                // Don't split when returning to/from the lobby or after rebooting the game
                return self.settings.ror1_stages && !(MENU_ROOMS.contains(&room.old) || MENU_ROOMS.contains(&room.current) || LOBBY_ROOMS.contains(&room.old) || LOBBY_ROOMS.contains(&room.current));
            }
        }
        return false;
    }

    /// Completed on reaching the outro cutscene
    ///
    /// Detects activating the console in room ID 41 (r6_1_1)
    fn completed(&self) -> bool {
        if let (Some(room), Some(run_end_flag)) = (self.game_state.room.pair, self.game_state.run_end_flag.pair) {
            return room.current == 41 && run_end_flag.changed_from_to(&0, &1);
        }
        return false;
    }

    /// No load removal (always false)
    fn is_loading(&self) -> Option<bool> { Some(false) }

}

/// Purely for documentation's sake
#[allow(non_camel_case_types)]
pub enum Room {
    /// White Screen
    rInit = 0,
    /// Hopoo Games logo
    rLogo,
    /// Main Menu
    rStart,
    /// Item Log
    rStorage,
    /// Monster Log
    rBook,
    /// Scores and Unlockables
    rHighscore,
    /// Single Player Lobby
    rSelect,
    /// Local Co-Op Lobby
    rSelectCoop,
    /// Unused?
    rIntro,
    /// Intro Cutscene
    rCutscene1,
    /// Intro Cutscene
    rCutscene2,
    /// Intro Cutscene
    rCutscene3,
    /// Intro Cutscene
    rCutscene4,
    /// Intro Cutscene
    rCutscene5,
    /// Intro Cutscene
    rCutscene6,
    /// Game Credits
    rCredits,
    /// Outro Cutscene
    r2Cutscene2,
    /// Outro Cutscene
    r2Cutscene3,
    r1_1_1, r1_1_2, r1_1_3,
    r1_2_1, r1_2_2, r1_2_3,
    r2_1_1, r2_1_2,
    r2_2_1, r2_2_2,
    r3_1_1, r3_1_2,
    rPigbeach,
    r3_2_1, r3_2_2,
    r4_1_1, r4_1_2,
    r4_2_1, r4_2_2,
    r5_1_1, r5_1_2,
    /// Online Co-Op Host/Join screen
    rHost,
    /// Online Co-Op Lobby
    rSelectMult,
    /// UES Contact Light
    r6_1_1,
}
