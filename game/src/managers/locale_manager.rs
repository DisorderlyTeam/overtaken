/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use engine::input::{ActionMapping, ControlType, InputManager};
use std::collections::HashMap;
use std::fs::{File, read_dir};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{info, warn};

pub struct LocaleManager {
    dir_path: PathBuf,
    languages: Vec<Language>,
    fallback_lang: Option<Language>,
    current_lang: Option<Language>,
    fallback_lang_data: Option<serde_json::Value>,
    current_lang_data: Option<serde_json::Value>,
}

impl Default for LocaleManager {
    fn default() -> Self {
        Self::new(Path::new("assets/lang"))
    }
}

impl LocaleManager {
    pub fn new(dir: &Path) -> LocaleManager {
        let mut lm = LocaleManager {
            dir_path: dir.to_owned(),
            languages: Self::fetch_languages(dir),
            fallback_lang: None,
            fallback_lang_data: None,
            current_lang: None,
            current_lang_data: None,
        };
        if let Some(fallback_lang) = lm.get_language("en").cloned() {
            lm.fallback_lang_data = lm.load_lang_data(&fallback_lang);
            lm.fallback_lang = Some(fallback_lang);
        }
        lm
    }

    pub fn fetch_languages(dir: &Path) -> Vec<Language> {
        read_dir(dir)
            .unwrap()
            .flat_map(|entry| {
                let entry = entry.ok()?;
                let lang = Language::from_file(&entry.path());

                if lang.is_none() {
                    warn!(
                        "Language file {} is invalid",
                        entry.file_name().to_str().unwrap()
                    );
                }

                lang
            })
            .collect()
    }

    pub fn languages(&self) -> &[Language] {
        &self.languages
    }

    pub fn get_language(&self, id: &str) -> Option<&Language> {
        self.languages.iter().find(|lang| lang.id == id)
    }

    fn load_lang_data(&self, lang: &Language) -> Option<serde_json::Value> {
        let file_path = self.dir_path.join(&lang.file_name);
        serde_json::from_reader(File::open(file_path).ok()?).ok()?
    }

    pub fn load_language(&mut self, id: &str) -> bool {
        let Some(lang) = Self::get_language(self, id) else {
            return false;
        };

        let Some(data) = self.load_lang_data(lang) else {
            return false;
        };

        self.current_lang = Some(lang.clone());
        self.current_lang_data = Some(data);

        info!("Loaded language: {:?}", self.current_lang);

        true
    }

    fn try_get_string_from(&self, path: &str, mut data: &serde_json::Value) -> Option<String> {
        let path_split = path.split('.');

        for key in path_split {
            data = data.get(key)?
        }
        data.as_str().map(|it| it.to_owned())
    }

    pub fn try_get_string(&self, path: &str) -> Option<String> {
        self.try_get_string_from(path, self.current_lang_data.as_ref()?)
            .or_else(|| self.try_get_string_from(path, self.fallback_lang_data.as_ref()?))
    }

    pub fn get_string(&self, path: &str) -> String {
        self.try_get_string(path)
            .unwrap_or_else(|| format!("[{}]", path))
    }

    pub fn get_string_ext(
        &self,
        path: &str,
        placeholders: impl Into<HashMap<String, String>>,
    ) -> String {
        let raw = self.get_string(path);

        let mut result = raw.to_string();
        for (key, value) in placeholders.into() {
            result = result.replace(&format!("{{{{{}}}}}", key), &value);
        }
        result
    }

    pub fn get_prompt_for_action(
        &self,
        input: &InputManager,
        action: &str,
        control_type: ControlType,
    ) -> String {
        let mapping = input
            .get_action_mappings(action)
            .into_iter()
            .flatten()
            .find(|mapping| mapping.matches_control_type(control_type));

        let gamepad_type_id = control_type
            .gamepad_type()
            .map_or_else(|| "generic", |gt| gt.id());

        match mapping {
            Some(ActionMapping::KeyboardKey(key)) => {
                if let Some(prompt) = self.try_get_string(&format!("keys.keyboard.{}", key.id())) {
                    return prompt;
                }

                self.get_string_ext(
                    "keys.keyboard.template",
                    [("key".into(), key.default_name().into())],
                )
            }
            Some(ActionMapping::GamepadButton(button)) => {
                self.get_string(&format!("keys.gamepad.{}.{}", gamepad_type_id, button.id()))
            }
            Some(ActionMapping::GamepadAxis { axis, .. }) => {
                self.get_string(&format!("keys.gamepad.{}.{}", gamepad_type_id, axis.id()))
            }
            None => self.get_string("keys.unmapped"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    id: String,
    file_name: String,
    language_name: String,
}

impl Language {
    pub fn from_file(file_path: &Path) -> Option<Language> {
        if file_path.extension()? != "json" {
            return None;
        }

        let id = file_path.file_stem()?.to_str()?.to_string();
        let file_name = file_path.file_name()?.to_str()?.to_string();

        let reader = File::open(file_path).unwrap();

        let data: serde_json::Value = serde_json::from_reader(reader).unwrap();

        let language_name = match data.get("_lang_name") {
            Some(name) => name.as_str()?.to_string(),
            None => {
                warn!(
                    "Language file {} does not contain _lang_name field",
                    file_name
                );
                id.clone()
            }
        };

        Some(Language {
            id,
            file_name,
            language_name,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn language_name(&self) -> &str {
        &self.language_name
    }
}

pub fn format_timer(time: Duration) -> String {
    let mut seconds = time.as_secs();
    let minutes = seconds / 60;
    seconds %= 60;
    format!(
        "{}:{:<02}.{:<02}",
        minutes,
        seconds,
        time.subsec_millis() / 10
    )
}
