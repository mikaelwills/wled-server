use std::fs;
use std::path::Path;

pub struct AudioFile;

impl AudioFile {
    pub fn parse_data_url(data_url: &str) -> Result<(String, String, Vec<u8>), Box<dyn std::error::Error>> {
        let url = data_url::DataUrl::process(data_url)
            .map_err(|e| format!("Invalid data URL: {}", e))?;

        let mime_type = url.mime_type().to_string();
        let extension = Self::mime_to_extension(&mime_type).to_string();

        let bytes = url
            .decode_to_vec()
            .map_err(|e| format!("Failed to decode base64: {:?}", e))?
            .0;

        Ok((mime_type, extension, bytes))
    }

    pub fn save(id: &str, data_url: &str, audio_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
        let (_mime_type, extension, bytes) = Self::parse_data_url(data_url)?;

        fs::create_dir_all(audio_path)?;

        let filename = format!("{}.{}", id, extension);
        let file_path = audio_path.join(&filename);

        fs::write(&file_path, &bytes)?;

        Ok(filename)
    }

    pub fn load(id: &str, audio_path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let extensions = ["mp3", "wav", "ogg", "flac"];
        let id = extensions.iter()
            .fold(id, |s, ext| s.strip_suffix(&format!(".{}", ext)).unwrap_or(s));

        for ext in &extensions {
            let file_path = audio_path.join(format!("{}.{}", id, ext));
            if file_path.exists() {
                return Ok(fs::read(&file_path)?);
            }
        }

        Err(format!("Audio file not found: {}", id).into())
    }

    pub fn delete(id: &str, audio_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let extensions = ["mp3", "wav", "ogg", "flac"];
        let id = extensions.iter()
            .fold(id, |s, ext| s.strip_suffix(&format!(".{}", ext)).unwrap_or(s));

        for ext in &extensions {
            let file_path = audio_path.join(format!("{}.{}", id, ext));
            if file_path.exists() {
                fs::remove_file(&file_path)?;
            }
        }

        let peaks_path = audio_path.join(format!("{}.peaks.json", id));
        if peaks_path.exists() {
            fs::remove_file(&peaks_path)?;
        }

        Ok(())
    }

    pub fn mime_to_extension(mime_type: &str) -> &str {
        match mime_type {
            "audio/mpeg" | "audio/mp3" => "mp3",
            "audio/wav" | "audio/wave" | "audio/x-wav" => "wav",
            "audio/ogg" => "ogg",
            "audio/flac" => "flac",
            _ => "mp3",
        }
    }

    pub fn extension_to_mime(filename: &str) -> &str {
        let ext = filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "mp3" => "audio/mpeg",
            "wav" => "audio/wav",
            "ogg" => "audio/ogg",
            "flac" => "audio/flac",
            _ => "application/octet-stream",
        }
    }
}
