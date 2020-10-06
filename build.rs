extern crate rae_shader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let shader_folders = vec!["src/shaders"];

    for shader_folder in shader_folders.iter() {
        let in_dir: std::path::PathBuf = [shader_folder, "glsl"].iter().collect();
        println!(
            "cargo:rerun-if-changed={}/**",
            in_dir.to_str().unwrap_or("")
        );
    }

    for shader_folder in shader_folders.iter() {
        let in_dir: std::path::PathBuf = [shader_folder, "glsl"].iter().collect();
        let out_dir: std::path::PathBuf = [shader_folder, "gen", "spirv"].iter().collect();
        rae_shader::compile_shaders_into_spirv(in_dir, out_dir)?;
    }
    Ok(())
}
