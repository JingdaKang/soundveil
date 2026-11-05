//! The recognition models on offer, lightest first.

/// Each raw value names a file in the upstream GGML collection and the file saved on disk, so
/// changing one orphans existing downloads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Model {
    Base,
    Small,
    TurboCompressed,
    #[default]
    Turbo,
    Quality,
}

impl Model {
    pub const ALL: [Self; 5] = [
        Self::Base,
        Self::Small,
        Self::TurboCompressed,
        Self::Turbo,
        Self::Quality,
    ];

    pub fn raw_value(self) -> &'static str {
        match self {
            Self::Base => "base.en",
            Self::Small => "small.en",
            Self::TurboCompressed => "large-v3-turbo-q5_0",
            Self::Turbo => "large-v3-turbo",
            Self::Quality => "large-v3",
        }
    }

    pub fn saved(value: Option<&str>) -> Self {
        Self::ALL
            .into_iter()
            .find(|model| Some(model.raw_value()) == value)
            .unwrap_or_default()
    }

    pub fn file_name(self) -> String {
        format!("ggml-{}.bin", self.raw_value())
    }

    pub fn download_url(self) -> String {
        format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            self.file_name()
        )
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Base => "Base",
            Self::Small => "Small",
            Self::TurboCompressed => "Large v3 Turbo Compressed",
            Self::Turbo => "Large v3 Turbo",
            Self::Quality => "Large v3",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Base => "Base · lightest · 148 MB",
            Self::Small => "Small · light · 488 MB",
            Self::TurboCompressed => "Large v3 Turbo Compressed · less memory · 574 MB",
            Self::Turbo => "Large v3 Turbo · recommended · 1.6 GB",
            Self::Quality => "Large v3 · heaviest · 3.1 GB",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Self::Base => {
                "The fastest choice for older computers or long battery life. Expect more mistakes, especially on names and fast speech."
            }
            Self::Small => {
                "More accurate than Base and still light. Try this if the Large models fall behind on your computer."
            }
            Self::TurboCompressed => {
                "Close to Turbo's accuracy in much less memory. A good first choice for computers with 8 GB."
            }
            Self::Turbo => {
                "The best balance of accuracy and speed. Needs about 2 GB of free memory, so it suits computers with 16 GB or more."
            }
            Self::Quality => {
                "May do better on difficult audio, but captions run further behind and it needs far more memory."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_installs_use_turbo() {
        assert_eq!(Model::saved(None), Model::Turbo);
    }

    #[test]
    fn unknown_saved_choice_falls_back_to_turbo() {
        assert_eq!(Model::saved(Some("large-v9")), Model::Turbo);
    }

    #[test]
    fn every_choice_survives_saving() {
        for model in Model::ALL {
            assert_eq!(Model::saved(Some(model.raw_value())), model);
        }
    }

    #[test]
    fn file_names_match_the_model_collection() {
        let names: Vec<String> = Model::ALL.iter().map(|model| model.file_name()).collect();
        assert_eq!(
            names,
            [
                "ggml-base.en.bin",
                "ggml-small.en.bin",
                "ggml-large-v3-turbo-q5_0.bin",
                "ggml-large-v3-turbo.bin",
                "ggml-large-v3.bin"
            ]
        );
    }
}
