use super::*;

#[test]
fn the_catalog_has_both_tabs_by_video_memory_with_files_it_can_check() {
    let all = catalog();
    for category in [Category::Uncensored, Category::Classic] {
        let tiers: Vec<u32> = all.iter().filter(|m| m.category == category).map(|m| m.tier_gb).collect();
        assert_eq!(tiers.len(), 5, "{category:?}");
        assert!(tiers.windows(2).all(|w| w[0] < w[1]), "{category:?} tiers go up: {tiers:?}");
    }
    for m in all {
        assert_eq!(all.iter().filter(|o| o.id == m.id).count(), 1, "{} is listed twice", m.id);
        assert!(m.file.to_ascii_lowercase().ends_with(".gguf") && !m.file.contains(['/', '\\']), "{}", m.file);
        assert!(m.sha256.as_deref().is_none_or(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())), "{}", m.id);
        assert!(m.download_url().starts_with("https://huggingface.co/") && m.download_url().ends_with(&m.file));
        assert!(m.runs_on_gb.is_none_or(|r| r < m.vram_gb), "{}: runs_on below vram", m.id);
    }
}

#[test]
fn a_model_fits_more_as_the_card_grows() {
    let inkling = catalog().iter().find(|m| m.preset.as_deref() == Some("looped-moe")).expect("the looped-MoE model is in the catalog");
    assert_eq!(inkling.fit(8 * 1024), Fit::No);
    assert_eq!(inkling.fit(16 * 1024), Fit::Partly, "a 16 GB card runs it with experts in system memory");
    assert_eq!(inkling.fit(32 * 1024), Fit::Fits);
}

#[test]
fn the_looped_preset_keeps_more_on_the_card_only_when_it_has_room() {
    let model = Path::new("Inkling-2-Mini-Uncensored-AD-Q4_K.gguf");
    assert_eq!(Preset::for_model(model), Preset::LoopedMoe);
    let mask = |free| {
        let a = launch_args(model, Preset::LoopedMoe, free, 8080);
        a.iter().position(|x| x == "-ot").and_then(|i| a.get(i + 1)).cloned().unwrap_or_default()
    };
    let roomy = mask(Some(LOOPED_FAST_FREE_MB + 100));
    let tight = mask(Some(LOOPED_FAST_FREE_MB - 100));
    assert_ne!(roomy, tight);
    assert!(tight.len() > roomy.len(), "the tight layout moves more blocks' experts off the card");
    assert_eq!(mask(None), tight, "unknown free memory takes the safe layout");
}

#[test]
fn every_launch_is_local_with_tool_templates_and_the_default_lets_llama_cpp_fit_it() {
    let model = Path::new("some-model.gguf");
    assert_eq!(Preset::for_model(model), Preset::Auto);
    let a = launch_args(model, Preset::Auto, None, 9000);
    let after = |flag: &str| a.iter().position(|x| x == flag).and_then(|i| a.get(i + 1)).cloned();
    assert_eq!(after("--host").as_deref(), Some("127.0.0.1"));
    assert_eq!(after("--fit").as_deref(), Some("on"));
    assert!(a.iter().any(|x| x == "--jinja"));
    assert!(!a.iter().any(|x| x == "-ngl"), "--fit gives up when the layers are set");
}

#[test]
fn gpus_are_read_from_nvidia_smi_lines() {
    let g = parse_gpus("NVIDIA GeForce RTX 4090 Laptop GPU, 16376, 16048\nbroken line\n");
    assert_eq!(g.len(), 1);
    assert!(g[0].free_mb < g[0].total_mb);
}

#[test]
fn the_newest_server_build_is_found_below_a_folder() {
    let d = tempfile::tempdir().expect("temp dir");
    for build in ["b12600", "b12722"] {
        let bin = d.path().join(build).join("build").join("bin");
        std::fs::create_dir_all(&bin).expect("dirs");
        std::fs::write(bin.join(server_exe()), b"").expect("file");
    }
    let found = find_server(d.path()).expect("found");
    assert!(found.starts_with(d.path().join("b12722")), "{}", found.display());
}

#[test]
fn bonsai_brings_its_sampling_and_needs_a_recent_engine() {
    let m = catalog().iter().find(|m| m.id == "classic-8").unwrap();
    assert!(m.file.ends_with("Q2_0.gguf"), "stock llama.cpp runs only the Q2_0 file");
    assert_eq!(m.min_build, Some(11556));
    let args = m.sampling.unwrap().args();
    assert_eq!(args, ["--temp", "1", "--top-p", "0.95", "--top-k", "20", "--min-p", "0"]);
}

#[test]
fn the_pinned_engine_runs_every_catalog_model() {
    let pinned: u32 = BUILD.trim_start_matches('b').parse().unwrap();
    for m in catalog() {
        assert!(m.min_build.is_none_or(|b| b <= pinned), "{} needs b{:?}, the engine is {BUILD}", m.id, m.min_build);
    }
}
