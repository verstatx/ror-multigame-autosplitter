use asr::{Address, game_engine::unity::{mono::Module, SceneManager, get_scene_name}, Error, future::{retry, next_tick}, PointerSize, Process, settings::{Gui, gui::Title}, string::{ArrayString}, watcher::Watcher};
use async_trait::async_trait;
use bytemuck::CheckedBitPattern;
use derive;

use crate::game;
use crate::AutoSplitter;

const TARGET_PROCESS_NAME : &str = "Risk of Rain 2.exe";

#[derive(Gui)]
pub struct GameSettings {
    /// Risk of Rain 2 Settings
    pub _ror2_settings: Title,
    /// Split on stage transitions
    ///
    /// This excludes selected hidden realms and game end conditions
    #[default = false]
    pub ror2_stages: bool,
    /// Split when leaving Bazaar Between Time
    #[default = false]
    pub bazaar: bool,
    /// Split when leaving Void Fields
    #[default = false]
    pub arena: bool,
    /// Split when leaving Gilded Shores
    #[default = false]
    pub goldshores: bool,
    /// Split when leaving Bulwark's Ambry
    #[default = false]
    pub artifactworld: bool,
}

/// Game state watchers
#[derive(Default)]
pub struct GameVars {
    /// FadeToBlackManager.alpha
    ///
    /// Value goes from 0.0->2.0 just before and during loads, then 2.0->0.0.
    pub fade: Watcher<f32>,
    /// Run.instance.stageClearCount
    ///
    /// Starts at 0 and increments on every regular stage, including after Commencement at the end of a run.
    /// Does not increment on special stages like Bazaar.
    pub stage_count: Watcher<i32>,
    /// GameOverController.instance.shouldDisplayGameEndReportPanels
    ///
    /// Invalid until a game end condition is reached (includes dying).
    pub results: Watcher<bool>,
    /// Unity scene name
    pub scene: Watcher<ArrayString<16>>,
}

/// MonoClass companion
struct StaticField<'a> {
    process: &'a Process,
    base_address: Address,
    field_offset: u64
}

impl StaticField<'_> {
    fn read_value<T: CheckedBitPattern>(&self) -> Result<T, Error> {
        return self.process.read_pointer_path::<T>(self.base_address, PointerSize::Bit64, &[0, self.field_offset]);
    }
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
    /// "Risk of Rain 2.exe"
    fn process_names(&self) -> &[&str] { return &[TARGET_PROCESS_NAME]; }

    fn register_settings(&mut self) {
        self.settings = GameSettings::register();
    }

    async fn attached(&mut self, process: &Process, autosplitter: &mut AutoSplitter) {
        self.reset_state();

        let monomod = Module::wait_attach_auto_detect(&process).await;
        let sceneman = SceneManager::wait_attach(&process).await;

        // Workaround for version detection: wait until the scene is valid
        // before attempting to load RoR2.dll/Assembly-CSharp.dll
        // FIXME replace with file check + wait_get_image
        retry(|| sceneman.get_current_scene_path::<256>(&process)).await;

        // SotV onwards uses RoR2.dll, earlier versions use Assembly-CSharp.dll
        // FIXME breaks version assumption if RoR2.dll has not yet loaded
        // check if file "RoR2.dll" exists once wasi support is merged.
        if let Some(ror2) = monomod.get_image(&process, "RoR2").or(monomod.get_default_image(&process)) {

            // FadeToBlackManager exists almost at the start of the process, but starts off invalid
            let mut ftbm = ror2.get_class(&process, &monomod, "FadeToBlackManager");
            // Run exists from entering the lobby onwards
            let mut run = ror2.get_class(&process, &monomod, "Run");
            // GameOverController exists just before the end of a run, including dying
            let mut goc = ror2.get_class(&process, &monomod, "GameOverController");
            // alpha valid when FadeToBlackManager exists
            let mut alpha_loc : Option<Address> = None;
            // stageClearCount only valid during a run (not valid in the lobby)
            let mut stage_loc : Option<StaticField> = None;
            // shouldDisplayGameEndReportPanels valid when GameOverController exists
            let mut panel_loc : Option<StaticField> = None;

            loop {
                // attmept to reload class fields when invalid
                if ftbm.is_none() {
                    ftbm = ror2.get_class(&process, &monomod, "FadeToBlackManager");
                    alpha_loc = None;
                }

                if run.is_none() {
                    run = ror2.get_class(&process, &monomod, "Run");
                    stage_loc = None;
                }

                if goc.is_none() {
                    goc = ror2.get_class(&process, &monomod, "GameOverController");
                    panel_loc = None;
                }

                if let Some(ftbm) = ftbm.as_ref() {
                    if alpha_loc.is_none() {
                        let alpha_offset = ftbm.get_field_offset(&process, &monomod, "alpha");
                        let alpha_addr = ftbm.get_static_table(&process, &monomod);
                        if let (Some(alpha_offset), Some(alpha_addr)) = (alpha_offset, alpha_addr) {
                            alpha_loc = Some(alpha_addr.add(alpha_offset.into()));
                        }
                    }
                }

                if let Some(run) = run.as_ref() {
                    if stage_loc.is_none() {
                        let instance_field = run.get_field_offset(&process, &monomod, "<instance>k__BackingField");
                        let scc_field = run.get_field_offset(&process, &monomod, "stageClearCount");
                        let static_table = run.get_static_table(&process, &monomod);
                        if let (Some(instance_field), Some(static_table), Some(scc_field)) = (instance_field, static_table, scc_field) {
                            let instance_addr = static_table.add(instance_field.into());
                            stage_loc = Some(StaticField{process: &process, base_address: instance_addr, field_offset: scc_field.into()})
                        }
                    }
                }

                if let Some(goc) = goc.as_ref() {
                    if panel_loc.is_none() {
                        let instance_field = goc.get_field_offset(&process, &monomod, "<instance>k__BackingField");
                        let sdgerp_field = goc.get_field_offset(&process, &monomod, "<shouldDisplayGameEndReportPanels>k__BackingField")
                            .or_else(|| goc.get_field_offset(&process, &monomod, "_shouldDisplayGameEndReportPanels")); // versions after SotS (starting with manifest 4567638355138669926 on 2024-08-27)
                        let static_table = goc.get_static_table(&process, &monomod);
                        if let (Some(instance_field), Some(static_table), Some(sdgerp_field)) = (instance_field, static_table, sdgerp_field) {
                            let instance_addr = static_table.add(instance_field.into());
                            panel_loc = Some(StaticField{process: &process, base_address: instance_addr, field_offset: sdgerp_field.into()})
                        }
                    }
                }

                // update game state watchers
                // make old = current when updating from an invalid state
                if alpha_loc.is_some() {
                    if self.game_state.fade.pair.is_none() {
                        self.game_state.fade.update( process.read::<f32>(alpha_loc.unwrap()).ok() );
                    }
                    self.game_state.fade.update( process.read::<f32>(alpha_loc.unwrap()).ok() );
                } else {
                    self.game_state.fade.update(None);
                }

                if let Some(stage_loc) = stage_loc.as_ref() {
                    if self.game_state.stage_count.pair.is_none() {
                        self.game_state.stage_count.update( stage_loc.read_value::<i32>().ok() );
                    }
                    self.game_state.stage_count.update( stage_loc.read_value::<i32>().ok() );
                } else {
                    self.game_state.stage_count.update(None);
                }

                if let Some(panel_loc) = panel_loc.as_ref() {
                    if self.game_state.results.pair.is_none() {
                        self.game_state.results.update( panel_loc.read_value::<bool>().ok() );
                    }
                    self.game_state.results.update( panel_loc.read_value::<bool>().ok() );
                } else {
                    self.game_state.results.update(None);
                }

                // update the scene name
                // skip scene name updates during scene transitions (always invalid)
                if let Some(scene) = sceneman.get_current_scene_path::<256>(&process).ok() {
                    let utf8_scene = std::str::from_utf8(get_scene_name(scene.as_bytes())).unwrap_or_default();
                    self.game_state.scene.update(ArrayString::<16>::from(&utf8_scene).ok());
                }

                self.settings.update();
                // cede control to main autosplitter logic loop
                autosplitter.update_loop(Some(self));
                next_tick().await;
            }
        }
    }

    /// Start on regular Stage 1s during fade-in
    fn start(&self) -> bool {
        if let (Some(scene), Some(fade)) = (self.game_state.scene.pair, self.game_state.fade.pair) {
            if scene.current.starts_with("golemplains") ||
               scene.current.starts_with("blackbeach") ||
               scene.current.starts_with("snowyforest") ||
               scene.current.starts_with("lakes") ||
               scene.current.starts_with("village")
            {
                return fade.current < 1.0 && fade.old >= 1.0;
            }
        }
        return false;
    }

    /// Reset on certain menu screens
    fn reset(&self) -> bool {
        if let Some(scene) = self.game_state.scene.pair {
            return match scene.current.as_str() {
                "lobby" | "title" | "crystalworld" | "eclipseworld" | "infinitetowerworld"
                    => true,
                _ => false
            }
        }
        return false;
    }

    /// Split on stage increment, and special scenes, ignoring game end conditions
    fn split(&self) -> bool {
        // stage count increased
        if self.settings.ror2_stages {
            if let Some(stage_count) = self.game_state.stage_count.pair {
                if stage_count.current >= 1 && stage_count.increased() {
                    // avoid double splits on Commencement
                    return match self.game_state.scene.pair {
                        Some(scene) => !scene.current.starts_with("moon"),
                        _ => true
                    }
                }
            }
        }
        if let Some(scene) = self.game_state.scene.pair {
            // reached a special scene
            if scene.changed() {
                match scene.old.as_str() {
                    "bazaar" => return self.settings.bazaar,
                    "arena" => return self.settings.arena,
                    "goldshores" => return self.settings.goldshores,
                    "artifactworld" => return self.settings.artifactworld,
                    _ => ()
                }
            }
        }
        return false;
    }

    /// Completed when the scene is the outro cutscene or if the game end was triggered for CE/SotV alt endings.
    fn completed(&self) -> bool {
        if let Some(scene) = self.game_state.scene.pair {
            if scene.changed() && scene.current.as_str() == "outro" {
                return true;
            }
            // completed a run on specific scenes
            if let Some(results) = self.game_state.results.pair {
                if results.changed_to(&true) {
                    match scene.current.as_str() {
                        "limbo" | "mysteryspace" | "voidraid" => return true,
                        _ => ()
                    }
                }
            }
        }
        return false;
    }

    /// Game is loading when FadeToBlackManager.alpha is increasing from 0->2.0 or at 2.0
    ///
    /// Sometimes this is undetermined when updates are too quick, or the game lags
    fn is_loading(&self) -> Option<bool> {
        if let Some(fade) = self.game_state.fade.pair {
            if fade.increased() {
                return Some(true);
            }
            if fade.decreased() && fade.current > 0.0 || fade.current == 0.0 {
                return Some(false);
            }
        }
        // maintain previous state when fade in/out is undetermined (aka current == previous)
        return None;
    }
}
