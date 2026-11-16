/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use engine::assets::AssetManager;
use engine::assets::texture::Texture;
use engine::core::vec2d::Array2D;
use engine::scene::{BlendMode, Material, Tileset};
use eyre::eyre;
use glam::{UVec2, Vec2};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum MaterialDef {
    Texture(String),
    Config(MaterialConfigDef),
}

#[derive(Deserialize, Clone, Debug)]
pub struct MaterialConfigDef {
    pub albedo: String,
    #[serde(default)]
    pub emission: Option<String>,
    #[serde(default)]
    pub alpha: bool,
}

impl MaterialDef {
    fn albedo(&self) -> &str {
        match self {
            Self::Texture(path) => path,
            Self::Config(config) => &config.albedo,
        }
    }

    fn blend_mode(&self) -> BlendMode {
        match self {
            Self::Texture(_) => BlendMode::Opaque,
            Self::Config(config) => {
                if config.alpha {
                    BlendMode::Alpha
                } else {
                    BlendMode::Opaque
                }
            }
        }
    }

    fn emission(&self) -> Option<&str> {
        match self {
            Self::Texture(_) => None,
            Self::Config(config) => config.emission.as_deref(),
        }
    }

    pub(crate) fn resolve_material(&self, assets: &mut AssetManager) -> eyre::Result<Material> {
        let albedo = assets.get_handle::<Texture>(&resolve_texture_path(self.albedo()))?;
        let emission = self
            .emission()
            .map(|path| assets.get_handle::<Texture>(&resolve_texture_path(path)))
            .transpose()?;
        Ok(Material::new(albedo, emission, self.blend_mode()))
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct LevelData {
    tileset: MaterialDef,
    collisions: String,
    tile_size: (usize, usize),
    map: Vec<Vec<i32>>,
    #[serde(default)]
    objects: Vec<LevelObjectDef>,
    #[serde(default)]
    vents: Option<VentLayerDef>,
    #[serde(default)]
    shadow_collision_mask: Option<u32>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
enum LevelMapDef {
    Inline(Vec<Vec<i32>>),
    CsvPath(String),
}

#[derive(Deserialize, Clone, Debug)]
pub struct VentLayerDef {
    tileset: MaterialDef,
    #[serde(default)]
    tile_size: Option<(usize, usize)>,
    map: LevelMapDef,
    #[serde(default)]
    collisions: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
struct RawLevelData {
    tileset: MaterialDef,
    collisions: String,
    tile_size: (usize, usize),
    map: LevelMapDef,
    #[serde(default)]
    objects: Vec<LevelObjectDef>,
    #[serde(default)]
    vents: Option<VentLayerDef>,
    #[serde(default)]
    shadow_collision_mask: Option<u32>,
}

#[derive(Debug)]
pub struct Level {
    pub tileset: Tileset,
    pub map: Array2D<i32>,
    pub objects: Vec<LevelObjectDef>,
    pub vent_layer: Option<VentLayer>,
    pub shadow_collision_mask: u32,
}

#[derive(Debug)]
pub struct VentLayer {
    pub tileset: Tileset,
    pub map: Array2D<i32>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LevelObjectDef {
    PlayerSpawn(PlayerSpawnObjectDef),
    Checkpoint(CheckpointObjectDef),
    Enemy(EnemyObjectDef),
    Obstacle(ObstacleObjectDef),
    VentEntrance(VentEntranceObjectDef),
    Trigger(TriggerObjectDef),
}

#[derive(Deserialize, Clone, Debug)]
pub struct PlayerSpawnObjectDef {
    pub position: (f32, f32),
}

#[derive(Deserialize, Clone, Debug)]
pub struct CheckpointObjectDef {
    pub position: (f32, f32),
}

#[derive(Deserialize, Clone, Debug)]
pub struct EnemyObjectDef {
    #[serde(default)]
    pub name: Option<String>,
    pub enemy_type: String,
    pub position: (f32, f32),
    pub patrol: Vec<PatrolStepDef>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ObstacleObjectDef {
    #[serde(default)]
    pub name: Option<String>,
    pub position: (f32, f32),
    pub material: MaterialDef,
    #[serde(default)]
    pub required_tags: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct VentEntranceObjectDef {
    #[serde(default)]
    pub name: Option<String>,
    pub position: (f32, f32),
    #[serde(default)]
    pub open: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct TriggerObjectDef {
    #[serde(default)]
    pub name: Option<String>,
    pub position: (f32, f32),
    pub size: (f32, f32),
    pub action: TriggerActionDef,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum TriggerActionDef {
    Named(TriggerNamedActionDef),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TriggerNamedActionDef {
    Win,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum PatrolStepDef {
    GoTo { go_to: (f32, f32) },
    Wait { wait: PatrolWaitDef },
}

#[derive(Deserialize, Clone, Debug)]
pub struct PatrolWaitDef {
    pub duration_s: f32,
    #[serde(default)]
    pub angle: Option<f32>,
}

impl LevelData {
    pub fn from_id(id: &str) -> eyre::Result<Self> {
        Self::from_file(&format!("assets/data/{}.json", id))
    }

    pub fn from_file(path: &str) -> eyre::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        let raw_data: RawLevelData = serde_json::from_str(&raw)?;
        let map = match raw_data.map {
            LevelMapDef::Inline(map) => map,
            LevelMapDef::CsvPath(csv_path) => {
                let resolved = resolve_data_path(&csv_path);
                parse_map_csv(&resolved)?
            }
        };
        let data = Self {
            tileset: raw_data.tileset,
            collisions: raw_data.collisions,
            tile_size: raw_data.tile_size,
            map,
            objects: raw_data.objects,
            vents: raw_data.vents,
            shadow_collision_mask: raw_data.shadow_collision_mask,
        };
        data.validate()?;
        Ok(data)
    }

    pub fn load(self, assets: &mut AssetManager) -> eyre::Result<Level> {
        let LevelData {
            tileset: tileset_def,
            collisions,
            tile_size,
            map: map_data,
            objects,
            vents: vent_layer_def,
            shadow_collision_mask,
        } = self;

        let material = tileset_def.resolve_material(assets)?;
        let albedo = material.texture;
        let tex_size_px = {
            let tex: &Texture = assets
                .resolve_handle(albedo)
                .ok_or_else(|| eyre!("Failed to load texture"))?;
            tex.size
        };

        let tile_count = UVec2::new(
            tex_size_px.x as u32 / tile_size.0 as u32,
            tex_size_px.y as u32 / tile_size.1 as u32,
        );
        let map_h = map_data.len();
        let map_w = map_data[0].len();

        let collisions_path = resolve_texture_path(&collisions);
        let collisions_text = std::fs::read_to_string(&collisions_path)?;
        let collisions = parse_collision_layers(&collisions_text, tile_count, &collisions_path)?;

        let tileset = Tileset {
            material,
            tile_size: Vec2::new(tile_size.0 as f32, tile_size.1 as f32),
            tile_count,
            collisions: Some(collisions),
        };

        let map = Array2D::new_init(map_w, map_h, |x, y| map_data[y][x]);

        let vent_layer = if let Some(vent_def) = vent_layer_def {
            let vent_map_rows = match vent_def.map {
                LevelMapDef::Inline(map) => map,
                LevelMapDef::CsvPath(path) => {
                    let resolved = resolve_data_path(&path);
                    parse_map_csv(&resolved)?
                }
            };
            let vent_height = vent_map_rows.len();
            let vent_width = vent_map_rows.get(0).map_or(0, |first| first.len());
            if vent_height != map_h || vent_width != map_w {
                return Err(eyre!(
                    "Vent map dimensions ({}, {}) differ from level map dimensions ({}, {})",
                    vent_width,
                    vent_height,
                    map_w,
                    map_h
                ));
            }
            if vent_map_rows.iter().any(|row| row.len() != vent_width) {
                return Err(eyre!("Vent map row width mismatch in 'vent_layer'"));
            }

            let vent_tile_size = vent_def.tile_size.unwrap_or(tile_size);
            let vent_material = vent_def.tileset.resolve_material(assets)?;
            let vent_albedo = vent_material.texture;
            let vent_tex_size_px = {
                let tex: &Texture = assets
                    .resolve_handle(vent_albedo)
                    .ok_or_else(|| eyre!("Failed to load texture"))?;
                tex.size
            };
            let vent_tile_count = UVec2::new(
                vent_tex_size_px.x as u32 / vent_tile_size.0 as u32,
                vent_tex_size_px.y as u32 / vent_tile_size.1 as u32,
            );

            let vent_collisions = if let Some(collisions_path) = &vent_def.collisions {
                let resolved = resolve_texture_path(collisions_path);
                let collisions_text = std::fs::read_to_string(&resolved)?;
                parse_collision_layers(&collisions_text, vent_tile_count, &resolved)?
            } else {
                let mut vent_collisions = Array2D::new_init(
                    vent_tile_count.x as usize,
                    vent_tile_count.y as usize,
                    |_, _| 0u32,
                );
                for row in &vent_map_rows {
                    for &tile_index in row {
                        if tile_index < 0 {
                            continue;
                        }
                        let tile_index = tile_index as u32;
                        let tx = (tile_index % vent_tile_count.x) as usize;
                        let ty = (tile_index / vent_tile_count.x) as usize;
                        if vent_collisions.is_within_bounds(tx, ty) {
                            vent_collisions.set(tx, ty, 1);
                        }
                    }
                }
                vent_collisions
            };
            let vent_tileset = Tileset {
                material: vent_material,
                tile_size: Vec2::new(vent_tile_size.0 as f32, vent_tile_size.1 as f32),
                tile_count: vent_tile_count,
                collisions: Some(vent_collisions),
            };

            let vent_map = Array2D::new_init(map_w, map_h, |x, y| vent_map_rows[y][x]);
            Some(VentLayer {
                tileset: vent_tileset,
                map: vent_map,
            })
        } else {
            None
        };

        Ok(Level {
            tileset,
            map,
            objects,
            vent_layer,
            shadow_collision_mask: shadow_collision_mask.unwrap_or(u32::MAX),
        })
    }

    fn validate(&self) -> eyre::Result<()> {
        if self.tile_size.0 == 0 || self.tile_size.1 == 0 {
            return Err(eyre!("tile_size must be non-zero"));
        }

        if self.map.is_empty() || self.map[0].is_empty() {
            return Err(eyre!("Map data is empty"));
        }

        if self.map.iter().any(|row| row.len() != self.map[0].len()) {
            return Err(eyre!("Map rows have inconsistent lengths"));
        }

        Ok(())
    }
}

fn resolve_data_path(path: &str) -> String {
    if path.starts_with("assets/") {
        path.to_string()
    } else {
        let joined: PathBuf = Path::new("assets/data").join(path);
        joined.to_string_lossy().into_owned()
    }
}

fn resolve_texture_path(path: &str) -> String {
    if path.starts_with("assets/") {
        path.to_string()
    } else {
        format!("assets/textures/{path}")
    }
}

fn parse_collision_layers(
    text: &str,
    expected_size: UVec2,
    path: &str,
) -> eyre::Result<Array2D<u32>> {
    if Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_none_or(|ext| ext != "col")
    {
        return Err(eyre!(
            "Unsupported collision file format for '{}'. Expected a .col file",
            path
        ));
    }

    let rows: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if rows.len() != expected_size.y as usize {
        return Err(eyre!(
            "Collision row count mismatch in '{}': expected {}, got {}",
            path,
            expected_size.y,
            rows.len()
        ));
    }

    let mut layers = Array2D::new(expected_size.x as usize, expected_size.y as usize, 0u32);
    for (y, row) in rows.iter().enumerate() {
        let cols: Vec<&str> = row.split(',').map(str::trim).collect();
        if cols.len() != expected_size.x as usize {
            return Err(eyre!(
                "Collision column count mismatch in '{}', row {}: expected {}, got {}",
                path,
                y,
                expected_size.x,
                cols.len()
            ));
        }

        for (x, value) in cols.iter().enumerate() {
            let layer = value.parse::<u32>().map_err(|err| {
                eyre!(
                    "Invalid collision value '{}' at ({}, {}) in '{}': {}",
                    value,
                    x,
                    y,
                    path,
                    err
                )
            })?;
            layers.set(x, y, layer);
        }
    }

    Ok(layers)
}

fn parse_map_csv(path: &str) -> eyre::Result<Vec<Vec<i32>>> {
    if Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_none_or(|ext| ext != "csv")
    {
        return Err(eyre!(
            "Unsupported map file format for '{}'. Expected a .csv file",
            path
        ));
    }

    let text = std::fs::read_to_string(path)?;
    let rows: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if rows.is_empty() {
        return Err(eyre!("Map CSV is empty in '{}'", path));
    }

    let mut map = Vec::with_capacity(rows.len());
    let mut expected_width: Option<usize> = None;
    for (y, row) in rows.iter().enumerate() {
        let cols: Vec<&str> = row.split(',').map(str::trim).collect();
        let width = cols.len();
        if width == 0 {
            return Err(eyre!("Map CSV row {} is empty in '{}'", y, path));
        }
        if let Some(expected) = expected_width {
            if width != expected {
                return Err(eyre!(
                    "Map CSV row width mismatch in '{}', row {}: expected {}, got {}",
                    path,
                    y,
                    expected,
                    width
                ));
            }
        } else {
            expected_width = Some(width);
        }

        let mut parsed_row = Vec::with_capacity(width);
        for (x, value) in cols.iter().enumerate() {
            let tile = value.parse::<i32>().map_err(|err| {
                eyre!(
                    "Invalid map value '{}' at ({}, {}) in '{}': {}",
                    value,
                    x,
                    y,
                    path,
                    err
                )
            })?;
            parsed_row.push(tile);
        }
        map.push(parsed_row);
    }

    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::{LevelData, LevelObjectDef, MaterialDef, PatrolStepDef, parse_map_csv};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn level_data_parses_material_and_enemy_objects() {
        let json = r#"
        {
          "tileset": {
            "albedo": "overtakentilesetnew.png",
            "emission": "overtakentilesetemis.png",
            "alpha": false
          },
          "collisions": "overtakentilesetnew.col",
          "tile_size": [16, 16],
          "map": [[0]],
          "objects": [
            {
              "type": "player_spawn",
              "position": [8.0, 9.0]
            },
            {
              "type": "enemy",
              "enemy_type": "minibot",
              "position": [10.0, 20.0],
              "patrol": [
                {"go_to": [10.0, 21.0]},
                {"wait": {"duration_s": 1.5}}
              ]
            }
          ]
        }
        "#;

        let data: LevelData = serde_json::from_str(json).expect("valid level data");
        match data.tileset {
            MaterialDef::Config(config) => {
                assert_eq!(config.albedo, "overtakentilesetnew.png");
                assert_eq!(config.emission.as_deref(), Some("overtakentilesetemis.png"));
                assert!(!config.alpha);
            }
            MaterialDef::Texture(path) => panic!("expected material config, got texture '{path}'"),
        }
        assert_eq!(data.objects.len(), 2);
        let LevelObjectDef::PlayerSpawn(player) = &data.objects[0] else {
            panic!("expected player_spawn object");
        };
        assert_eq!(player.position, (8.0, 9.0));
        let LevelObjectDef::Enemy(enemy) = &data.objects[1] else {
            panic!("expected enemy object");
        };
        assert_eq!(enemy.enemy_type, "minibot");
        assert_eq!(enemy.position, (10.0, 20.0));
        assert_eq!(enemy.patrol.len(), 2);
        match enemy.patrol[0] {
            PatrolStepDef::GoTo { go_to } => assert_eq!(go_to, (10.0, 21.0)),
            _ => panic!("expected go_to step"),
        }
    }

    #[test]
    fn level_data_from_file_supports_csv_map_path() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("valid system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("level_map_{unique}.csv"));
        std::fs::write(&path, "1,2\n3,4\n").expect("write temp csv");

        let map = parse_map_csv(path.to_str().expect("utf-8 path")).expect("valid csv");
        assert_eq!(map, vec![vec![1, 2], vec![3, 4]]);

        std::fs::remove_file(path).expect("cleanup temp csv");
    }

    #[test]
    fn level_data_parses_checkpoint_with_only_position() {
        let json = r#"
        {
          "tileset": "overtakentilesetnew.png",
          "collisions": "overtakentilesetnew.col",
          "tile_size": [16, 16],
          "map": [[0]],
          "objects": [
            {
              "type": "player_spawn",
              "position": [8.0, 9.0]
            },
            {
              "type": "checkpoint",
              "position": [304.0, 560.0]
            }
          ]
        }
        "#;

        let data: LevelData = serde_json::from_str(json).expect("valid checkpoint object");
        assert_eq!(data.objects.len(), 2);
        let LevelObjectDef::Checkpoint(checkpoint) = &data.objects[1] else {
            panic!("expected checkpoint object");
        };
        assert_eq!(checkpoint.position, (304.0, 560.0));
    }
}
