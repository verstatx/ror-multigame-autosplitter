use asr::{future::{next_tick, retry}, Process, settings::{Gui, gui::Title}, watcher::Watcher};
use async_trait::async_trait;
use derive;

#[cfg(debug_output)]
use { asr::timer, std::fmt };

use crate::game;
use crate::AutoSplitter;

use version_details::*;

const TARGET_PROCESS_NAME : &str = "Risk of Rain Returns.exe";

#[derive(Gui)]
pub struct GameSettings {
    /// Risk of Rain Returns Settings
    pub _rorr_settings: Title,
    /// Split on stage transitions
    #[default = false]
    pub rorr_stages: bool,
}

/// Game state watchers
#[derive(Default)]
pub struct GameVars {
    /// GameMaker room ID
    pub room: Watcher<i32>,
    /// Time Alive
    pub in_game_time: Watcher<f64>,
}

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

#[async_trait]
impl game::GameAutoSplitter for Game {
    /// "Risk of Rain Returns.exe"
    fn process_names(&self) -> &[&str] { return &[TARGET_PROCESS_NAME]; }

    fn register_settings(&mut self) {
        self.settings = GameSettings::register();
    }

    async fn attached(&mut self, process: &Process, autosplitter: &mut AutoSplitter) {
        self.reset_state();

        let (main_module, _main_module_size) = process.wait_module_range(&TARGET_PROCESS_NAME).await; // slow, but avoids deadlock

        // Log main module size (differs on Linux)
        #[cfg(debug_output)] asr::print_message(&_main_module_size.to_string());

        // game version detection and handling
        let (room, in_game_time) = retry(|| find_gamevar_pointers(process, &main_module)).await; // intentionally hangs for unsupported versions

        loop {
            // update game state watchers
            self.game_state.room.update(
                match room.deref::<i32>(&process) {
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
                    Some(room) => timer::set_variable("[RoR:R] room ID", &format!("{0:?}", room.current)),
                    _ => timer::set_variable("[RoR:R] room ID", "[invalid]")
                }
                match self.game_state.in_game_time.pair {
                    Some(in_game_time) => timer::set_variable("[RoR:R] In-Game Time", &format!("{0:?}", in_game_time.current)),
                    _ => timer::set_variable("[RoR:R] In-Game Time", "[invalid]")
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

    /// Start when entering a game from the lobby
    fn start(&self) -> bool {
        if let Some(room) = self.game_state.room.pair {
            return room.changed_from(&4) &&
                match room.current {
                    2 | 3 | 4 => false,
                    _ => true
                };
        }
        return false;
    }

    /// Reset when entering the lobby
    fn reset(&self) -> bool {
        if let Some(room) = self.game_state.room.pair {
            return room.current == 4;
        }
        return false;
    }

    /// Split on stage change
    fn split(&self) -> bool {
        const MENU_ROOMS : [i32; 5] = [1, 2, 3, 4, 7];

        // Stage/room changed
        if let Some(room) = self.game_state.room.pair {
            if room.changed() {
                // Don't split when returning to the lobby
                return self.settings.rorr_stages && !(MENU_ROOMS.contains(&room.old) || MENU_ROOMS.contains(&room.current));
            }
        }
        return false;
    }

    /// Completed on reaching the outro cutscene
    fn completed(&self) -> bool {
        if let Some(room) = self.game_state.room.pair {
            if room.changed() && room.current == 8 {
                return true;
            }
        }
        return false;
    }

    /// No load removal
    fn is_loading(&self) -> Option<bool> { Some(false) }

}

mod version_details {
    use asr::{Address, deep_pointer::DeepPointer, Process};

// public interface

    /// Guaranteed to be large enough to hold a DeepPointer to "room" from any version
    pub type RoomPointer = DeepPointer::<{SupportedGameVersions::room_len()}>;
    /// Guaranteed to be large enough to hold a DeepPointer to "in_game_time" from any version
    pub type IGTPointer = DeepPointer::<{SupportedGameVersions::igt_len()}>;

    /// Autodetects game version and locates offsets for game vars
    pub fn find_gamevar_pointers<'a>(process: &'a Process, module_offset: &'a Address) -> Option<(RoomPointer, IGTPointer)> {
        for gv in SupportedGameVersions::data() {
            if check_build_string(process, module_offset, &gv.build_string) {
                // Log detected version
                #[cfg(debug_output)] asr::print_message(&format!("{}", gv.version));
                //return Some(&gv.offsets);
                return Some((RoomPointer::new_64bit(*module_offset, gv.offsets.room), IGTPointer::new_64bit(*module_offset, gv.offsets.in_game_time)));
            }
        }
        return None;
    }

// implementation details

    /// Version specific build info used for version detection
    struct BuildString {
        address: u64,
        expected: &'static str,
    }

    /// Version specific pointer offsets to game vars
    struct GameVarOffsets {
        pub room: &'static [u64],
        pub in_game_time: &'static [u64],
    }

    #[cfg(debug_output)]
    #[repr(u32)] #[derive(Clone, Copy)]
    pub enum GameVersion {
        V1_0_1,
        V1_0_2,
        V1_0_3,
        V1_0_4,
        V1_0_5,
    }

    #[cfg(debug_output)]
    impl fmt::Display for GameVersion {
        /// FIXME lazy fragile hack
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "v1.0.{:?}", (*self as u32) + 1)
        }
    }

    struct GameVersionData {
        #[cfg(debug_output)] version: GameVersion,
        build_string: BuildString,
        offsets: GameVarOffsets,
    }

    /// Holds static data for each game version the autosplitter supports
    struct SupportedGameVersions;

    impl SupportedGameVersions {
        /// Autosplitter reference data for every supported version
        const fn data() -> &'static [GameVersionData] { return &Self::VERSION_DATA; }
        /// size of longest BuildString
        const fn strbuf_len() -> usize { return Self::max_len_all().0; }
        /// size of longest room pointer path
        const fn room_len() -> usize { return Self::max_len_all().1; }
        /// size of longest in_game_time pointer path
        const fn igt_len() -> usize { return Self::max_len_all().2; }

        const VERSION_DATA: [GameVersionData; 3] = [
            { GameVersionData {
                #[cfg(debug_output)] version: GameVersion::V1_0_3,
                build_string: { BuildString {
                    address: 0x1A7C700,
                    expected: "BUILD_ID: 234, BUILD_BRANCH: PATCH_1_0_3, VERSION_STRING: 1.0.3"
                } },
                offsets: { GameVarOffsets {
                    room: &[0x2127B18],
                    in_game_time: &[0x1F01C98, 0x10, 0x1CF0, 0x1B0, 0x48, 0x10, 0x0, 0x0, 0x48, 0x10, 0x50, 0x0]
                } }
            } },

            { GameVersionData {
                #[cfg(debug_output)] version: GameVersion::V1_0_4,
                build_string: { BuildString {
                    address: 0x1ABCB10,
                    expected: "BUILD_ID: 242, BUILD_BRANCH: the-mouse-aim-branch, VERSION_STRING: 1.0.4"
                } },
                offsets: { GameVarOffsets {
                    room: &[0x2172888],
                    in_game_time: &[0x01F5F300, 0x170, 0x10, 0x90, 0x0, 0x48, 0x10, 0x60, 0x0, 0x48, 0x10, 0x1B0, 0x0]
                } }
            } },

            { GameVersionData {
                #[cfg(debug_output)] version: GameVersion::V1_0_5,
                build_string: { BuildString {
                    address: 0x1ABC988,
                    expected: "BUILD_ID: 248, BUILD_BRANCH: master, VERSION_STRING: 1.0.4"
                } },
                offsets: { GameVarOffsets {
                    room: &[0x21729D8],
                    in_game_time: &[0x01F5F450, 0x120, 0x10, 0x90, 0x0, 0x48, 0x10, 0xd0, 0x0, 0x48, 0x10, 0x2e0, 0x0]
                } }
            } },
        ];

        /// utility function for constant evaluation contexts
        const fn max_len_all() -> (usize, usize, usize) {
            let mut max_build_str: usize = 0;
            let mut max_room: usize = 0;
            let mut max_igt: usize = 0;

            let mut i = 0; while i < Self::VERSION_DATA.len() {
                let build_str_len = Self::VERSION_DATA[i].build_string.expected.len();
                let room_len = Self::VERSION_DATA[i].offsets.room.len();
                let igt_len = Self::VERSION_DATA[i].offsets.in_game_time.len();

                if max_build_str < build_str_len {
                    max_build_str = build_str_len;
                }
                if max_room < room_len {
                    max_room = room_len;
                }
                if max_igt < igt_len {
                    max_igt = igt_len;
                }

                i += 1;
            }

            return (max_build_str, max_room, max_igt);
        }
    }

    fn check_build_string(process: &Process, module_offset: &Address, build_string: &'static BuildString) -> bool {
        let mut buf: [u8; SupportedGameVersions::strbuf_len()] = [0; SupportedGameVersions::strbuf_len()];
        if process.read_into_buf(module_offset.add(build_string.address), &mut buf).is_ok() {
            return buf[0..build_string.expected.len()].iter().zip(build_string.expected.as_bytes().iter()).all(|(a,b)| a == b);
        }
        return false;
    }

}
