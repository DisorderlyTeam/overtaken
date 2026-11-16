/*
 * Copyright (c) 2026. DisorderlyTeam
 */

pub mod assets;
pub mod audio;
pub mod context;
pub mod core;
pub mod engine;
pub mod game_director;
pub mod input;
pub mod prelude;
pub mod render_world;
pub mod renderer;
pub mod scene;

pub fn hello() -> &'static str {
    "Hello, library!"
}

#[cfg(test)]
mod tests {
    use super::*;
    use prelude::test;
    use tracing::info;

    #[test]
    fn test() {
        info!("Meow!!!");
        assert_eq!(hello(), "Hello, library!");
    }
}
