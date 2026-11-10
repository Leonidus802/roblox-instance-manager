use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Game {
    pub name: String,
    pub place_id: u64,
    #[serde(default)]
    pub server_id: String,
}

impl Game {
    pub fn new(name: &str, place: &str, server: &str) -> Result<Self, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
            return Err("Enter a game name between 1 and 80 characters.".into());
        }
        let place = place.trim();
        if place.is_empty() || !place.bytes().all(|b| b.is_ascii_digit()) {
            return Err("Place ID must contain only digits (from the game's /games/ URL).".into());
        }
        let place_id = place
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or("Place ID must be a positive number within the 64-bit range.")?;
        let server_id = server.trim().to_owned();
        if !server_id.is_empty() && !valid_uuid(&server_id) {
            return Err("Server ID must be a JobId in UUID form, or left empty.".into());
        }
        Ok(Self {
            name: name.into(),
            place_id,
            server_id,
        })
    }

    pub fn launch_uri(&self) -> String {
        let mut uri = format!("roblox://placeId={}", self.place_id);
        if !self.server_id.is_empty() {
            uri.push_str("&gameInstanceId=");
            uri.push_str(&self.server_id);
        }
        uri
    }

    pub fn web_url(&self) -> String {
        if self.server_id.is_empty() {
            format!("https://www.roblox.com/games/{}", self.place_id)
        } else {
            format!(
                "https://www.roblox.com/games/start?placeId={}&gameInstanceId={}",
                self.place_id, self.server_id
            )
        }
    }
}

fn valid_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub games: Vec<Game>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("Cannot read saved games: {e}")),
        };
        let config: Self = serde_json::from_slice(&bytes).map_err(|e| {
            format!("Saved games file is invalid: {e}. The file has been preserved.")
        })?;
        for g in &config.games {
            Game::new(&g.name, &g.place_id.to_string(), &g.server_id)?;
        }
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let parent = path.parent().ok_or("Invalid settings path")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let temp = path.with_extension("json.tmp");
        let mut file = fs::File::create(&temp).map_err(|e| e.to_string())?;
        use std::io::Write;
        file.write_all(&data)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        // Same-directory replacement; Windows rename cannot replace existing files.
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            #[link(name = "kernel32")]
            extern "system" {
                fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
            }
            let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 1 | 8) } == 0 {
                return Err(io::Error::last_os_error().to_string());
            }
        }
        #[cfg(not(windows))]
        fs::rename(temp, path).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validated_launch_links_cannot_inject_parameters() {
        assert!(Game::new("Test", "123&userId=9", "").is_err());
        assert!(Game::new("Test", "123", "&accessCode=secret").is_err());
        assert!(Game::new("Test", "0", "").is_err());
        assert!(Game::new("Test", "18446744073709551616", "").is_err());
        let game = Game::new(" My game ", "123", "01234567-89ab-cdef-0123-456789abcdef").unwrap();
        assert_eq!(
            game.launch_uri(),
            "roblox://placeId=123&gameInstanceId=01234567-89ab-cdef-0123-456789abcdef"
        );
        assert!(game
            .web_url()
            .starts_with("https://www.roblox.com/games/start?"));
    }
    #[test]
    fn config_round_trip_replace_and_corruption() {
        let dir = std::env::temp_dir().join(format!("rim-tests-{}", std::process::id()));
        let path = dir.join("games.json");
        assert!(Config::load(&path).unwrap().games.is_empty());
        let mut config = Config {
            games: vec![Game::new("日本語", "123", "").unwrap()],
        };
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap().games, config.games);
        config.games.push(Game::new("Second", "456", "").unwrap());
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap().games.len(), 2);
        fs::write(&path, b"{broken").unwrap();
        assert!(Config::load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{broken");
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
