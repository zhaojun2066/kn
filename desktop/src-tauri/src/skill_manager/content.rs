use std::path::Path;

use super::types::SkillContent;

/// Read a skill file and extract its description + body content.
pub(super) fn read_skill_content(path: String) -> Result<SkillContent, String> {
    if path.is_empty() {
        return Ok(SkillContent {
            description: String::new(),
            body: String::new(),
        });
    }

    let file_path = Path::new(&path);
    let actual_path = if file_path.is_dir() {
        let md = file_path.join("SKILL.md");
        if md.exists() {
            md
        } else {
            // Check for disabled skill (SKILL.md.disabled) before falling back to skill.md
            let md_disabled = file_path.join("SKILL.md.disabled");
            if md_disabled.exists() {
                md_disabled
            } else {
                file_path.join("skill.md")
            }
        }
    } else {
        file_path.to_path_buf()
    };

    let content = std::fs::read_to_string(&actual_path).map_err(|e| format!("读取失败: {}", e))?;

    let mut lines = content.lines();
    let mut description = String::new();
    let mut body = String::new();
    let mut in_frontmatter = false;
    let mut frontmatter_done = false;
    let mut frontmatter_count = 0;

    for line in &mut lines {
        if !frontmatter_done {
            if line.trim() == "---" {
                frontmatter_count += 1;
                if frontmatter_count == 1 {
                    in_frontmatter = true;
                    continue;
                } else if frontmatter_count == 2 {
                    frontmatter_done = true;
                    continue;
                }
            }
            if in_frontmatter {
                if let Some(val) = line.strip_prefix("description:") {
                    description = val.trim().to_string();
                }
                continue;
            }
            if frontmatter_count == 0 {
                frontmatter_done = true;
                body.push_str(line);
                body.push('\n');
            }
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }

    Ok(SkillContent {
        description,
        body: body.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_directory_skill_content_with_frontmatter() {
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp.path().join("demo");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Demo skill\n---\n\nBody text\n",
        )
        .expect("write skill");

        let content = read_skill_content(skill_dir.to_string_lossy().to_string()).expect("read");

        assert_eq!(content.description, "Demo skill");
        assert_eq!(content.body, "Body text");
    }

    #[test]
    fn reads_disabled_directory_skill_content() {
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp.path().join("demo");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(skill_dir.join("SKILL.md.disabled"), "Disabled body\n")
            .expect("write skill");

        let content = read_skill_content(skill_dir.to_string_lossy().to_string()).expect("read");

        assert_eq!(content.description, "");
        assert_eq!(content.body, "Disabled body");
    }
}
