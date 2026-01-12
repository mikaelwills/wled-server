use std::fs;
use std::path::Path;

/// Audio file storage and management
pub struct AudioFile;

impl AudioFile {
    /// Parse a data URL and extract MIME type, extension, and decoded bytes
    ///
    /// Example: "data:audio/mp3;base64,ABC123..." => ("audio/mp3", "mp3", [binary data])
    pub fn parse_data_url(data_url: &str) -> Result<(String, String, Vec<u8>), Box<dyn std::error::Error>> {
        // Parse using data-url crate
        let url = data_url::DataUrl::process(data_url)
            .map_err(|e| format!("Invalid data URL: {}", e))?;

        let mime_type = url.mime_type().to_string();
        let extension = Self::mime_to_extension(&mime_type).to_string();

        // Decode base64 body
        let bytes = url.decode_to_vec()
            .map_err(|e| format!("Failed to decode base64: {:?}", e))?
            .0;

        Ok((mime_type, extension, bytes))
    }

    /// Save audio data from a data URL to a file
    ///
    /// Returns the filename (e.g., "program-123.mp3")
    pub fn save(id: &str, data_url: &str, audio_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
        // Create audio directory if it doesn't exist
        fs::create_dir_all(audio_path)?;

        // Parse data URL and extract bytes
        let (mime_type, extension, bytes) = Self::parse_data_url(data_url)?;

        tracing::info!("Saving audio file: id={}, mime={}, size={}bytes", id, mime_type, bytes.len());

        // Generate filename
        let filename = format!("{}.{}", id, extension);
        let file_path = audio_path.join(&filename);

        // Write binary data to file
        fs::write(&file_path, bytes)?;

        tracing::info!("Audio file saved: {}", file_path.display());
        Ok(filename)
    }

    /// Load audio file as binary data
    pub fn load(filename: &str, audio_path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Validate filename (prevent path traversal)
        if filename.contains("..") || filename.contains("/") || filename.contains("\\") {
            return Err("Invalid filename: path traversal detected".into());
        }

        let file_path = audio_path.join(filename);

        if !file_path.exists() {
            return Err(format!("Audio file not found: {}", filename).into());
        }

        let bytes = fs::read(&file_path)?;
        tracing::info!("Loaded audio file: {}, size={}bytes", filename, bytes.len());

        Ok(bytes)
    }

    /// Delete an audio file and its associated peaks file
    pub fn delete(filename: &str, audio_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        // Validate filename (prevent path traversal)
        if filename.contains("..") || filename.contains("/") || filename.contains("\\") {
            return Err("Invalid filename: path traversal detected".into());
        }

        let file_path = audio_path.join(filename);

        if file_path.exists() {
            fs::remove_file(&file_path)?;
            tracing::info!("Deleted audio file: {}", filename);
        } else {
            tracing::warn!("Audio file not found for deletion: {}", filename);
        }

        // Also delete associated peaks file if it exists
        let peaks_path = audio_path.join(format!("{}.peaks.json", filename));
        if peaks_path.exists() {
            fs::remove_file(&peaks_path)?;
            tracing::info!("Deleted peaks file: {}.peaks.json", filename);
        }

        Ok(())
    }

    /// Map MIME type to file extension
    fn mime_to_extension(mime: &str) -> &str {
        match mime {
            "audio/wav" | "audio/x-wav" => "wav",
            "audio/webm" => "webm",
            "audio/mp3" | "audio/mpeg" => "mp3",
            "audio/ogg" => "ogg",
            "audio/flac" => "flac",
            _ => "bin", // Fallback for unknown types
        }
    }

    /// Infer MIME type from file extension
    pub fn extension_to_mime(filename: &str) -> &str {
        if filename.ends_with(".wav") {
            "audio/wav"
        } else if filename.ends_with(".webm") {
            "audio/webm"
        } else if filename.ends_with(".mp3") {
            "audio/mpeg"
        } else if filename.ends_with(".ogg") {
            "audio/ogg"
        } else if filename.ends_with(".flac") {
            "audio/flac"
        } else {
            "application/octet-stream"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_to_extension() {
        assert_eq!(AudioFile::mime_to_extension("audio/mp3"), "mp3");
        assert_eq!(AudioFile::mime_to_extension("audio/wav"), "wav");
        assert_eq!(AudioFile::mime_to_extension("audio/webm"), "webm");
    }

    #[test]
    fn test_extension_to_mime() {
        assert_eq!(AudioFile::extension_to_mime("test.mp3"), "audio/mpeg");
        assert_eq!(AudioFile::extension_to_mime("test.wav"), "audio/wav");
        assert_eq!(AudioFile::extension_to_mime("test.webm"), "audio/webm");
    }

    #[test]
    fn test_path_traversal_prevention() {
        use std::path::PathBuf;
        let audio_path = PathBuf::from("/tmp/audio");

        // These should fail
        assert!(AudioFile::load("../etc/passwd", &audio_path).is_err());
        assert!(AudioFile::delete("../../secrets.txt", &audio_path).is_err());
    }
}
