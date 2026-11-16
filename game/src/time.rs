/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use std::fmt::{Debug, Formatter};
use std::time::Duration;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct GameTime {
    centiseconds: u32,
}

impl From<Duration> for GameTime {
    fn from(value: Duration) -> Self {
        GameTime {
            centiseconds: value
                .as_secs()
                .try_into()
                .unwrap_or(u32::MAX)
                .saturating_mul(100)
                + value.subsec_millis() / 10,
        }
    }
}

impl From<GameTime> for Duration {
    fn from(value: GameTime) -> Self {
        Duration::from_millis(value.centiseconds as u64 * 10)
    }
}

impl Debug for GameTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{:02}s",
            self.centiseconds / 100,
            self.centiseconds % 100
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::time::GameTime;
    use std::time::Duration;

    #[test]
    fn test_time() {
        let time = GameTime::from(Duration::from_millis(1234));
        assert_eq!(time.centiseconds, 123);
        let dur: Duration = time.into();
        assert_eq!(dur, Duration::from_millis(1230));
    }

    #[test]
    fn test_time_serialize() {
        let time = GameTime::from(Duration::from_millis(12345));

        let serialized = serde_json::to_string(&time).unwrap();
        assert_eq!(serialized, "1234");

        let deserialized = serde_json::from_str::<GameTime>(&serialized).unwrap();

        assert_eq!(time, deserialized);
    }
}
