//! 배포 설정이 어긋나지 않는지 지키는 시험 (요구사항 §11·§15).
//!
//! 과거에 겪은 문제들 — `latest.json` 누락, updater artifact 누락, 서명 불일치,
//! 세 곳의 버전 불일치 — 는 모두 **빌드 전에 파일만 봐도 알 수 있는 것**이다.
//! 그래서 코드가 아니라 설정을 시험한다.

use serde_json::Value;

fn tauri_conf() -> Value {
    serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json 파싱")
}

fn package_json() -> Value {
    serde_json::from_str(include_str!("../../package.json")).expect("package.json 파싱")
}

#[test]
fn 버전이_세_곳에서_같다() {
    let cargo = env!("CARGO_PKG_VERSION");
    let conf = tauri_conf();
    let pkg_json = package_json();
    let tauri = conf["version"].as_str().expect("tauri.conf.json version");
    let pkg = pkg_json["version"].as_str().expect("package.json version");

    assert_eq!(
        cargo, tauri,
        "Cargo.toml({cargo})과 tauri.conf.json({tauri})의 버전이 다릅니다. \
         npm run version:set 으로 맞추세요."
    );
    assert_eq!(
        cargo, pkg,
        "Cargo.toml({cargo})과 package.json({pkg})의 버전이 다릅니다. \
         npm run version:set 으로 맞추세요."
    );
}

#[test]
fn 버전_형식이_updater가_읽을_수_있는_모양이다() {
    let v = env!("CARGO_PKG_VERSION");
    let parts: Vec<&str> = v.split('.').collect();
    assert_eq!(parts.len(), 3, "버전은 x.y.z 세 자리여야 합니다: {v}");
    for p in parts {
        assert!(
            p.parse::<u32>().is_ok(),
            "버전의 각 자리는 숫자여야 합니다: {v}"
        );
    }
}

#[test]
fn updater_설정이_빠지지_않았다() {
    let conf = tauri_conf();
    let up = &conf["plugins"]["updater"];
    assert!(!up.is_null(), "plugins.updater 설정이 없습니다.");

    let endpoints = up["endpoints"].as_array().expect("updater.endpoints");
    assert!(!endpoints.is_empty(), "updater 확인 주소가 비어 있습니다.");
    let first = endpoints[0].as_str().unwrap_or_default();
    assert!(
        first.ends_with("latest.json"),
        "updater 주소가 latest.json으로 끝나야 합니다: {first}"
    );
    assert!(
        first.starts_with("https://"),
        "updater 주소는 https여야 합니다: {first}"
    );

    let pubkey = up["pubkey"].as_str().unwrap_or_default();
    assert!(!pubkey.is_empty(), "updater 공개키가 없습니다.");
    assert!(
        pubkey.len() > 100,
        "updater 공개키가 잘린 것 같습니다 (길이 {})",
        pubkey.len()
    );
}

#[test]
fn updater_배포물을_만들도록_되어_있다() {
    // 이것이 false면 설치파일은 나오지만 .sig와 latest.json이 없어
    // "설치는 됐지만 업데이트 실패"가 된다
    let conf = tauri_conf();
    assert_eq!(
        conf["bundle"]["createUpdaterArtifacts"].as_bool(),
        Some(true),
        "bundle.createUpdaterArtifacts가 true여야 updater 배포물이 만들어집니다."
    );
}

#[test]
fn 윈도우_설치파일_설정이_있다() {
    let conf = tauri_conf();
    let targets = conf["bundle"]["targets"].as_array().expect("bundle.targets");
    assert!(
        targets.iter().any(|t| t.as_str() == Some("nsis")),
        "Windows 설치파일(nsis)이 대상에 없습니다."
    );
    let nsis = &conf["bundle"]["windows"]["nsis"];
    assert_eq!(
        nsis["installMode"].as_str(),
        Some("currentUser"),
        "관리자 권한 없이 설치되도록 currentUser여야 합니다."
    );
}

#[test]
fn 한글_이름_설치파일을_영문으로_바꿔_올린다() {
    // 앱 이름이 한글이면 makensis가 만드는 파일 이름도 한글이 된다. GitHub
    // Release는 첨부 파일 이름에서 한글을 떼어 내므로, latest.json의 url과
    // 실제 첨부 파일 주소가 어긋나 "받는 중"에서 업데이트가 실패한다.
    // 그래서 워크플로가 영문 이름으로 복사한 뒤 latest.json을 직접 만든다.
    let conf = tauri_conf();
    let product = conf["productName"].as_str().unwrap_or_default();
    if product.is_ascii() {
        return; // 영문 이름이면 이 문제가 없다
    }

    let wf = include_str!("../../.github/workflows/release.yml");
    assert!(
        wf.contains("afterschool_${ver}_x64-setup.exe"),
        "앱 이름이 한글({product})인데 워크플로가 영문 이름으로 바꾸지 않습니다."
    );
    assert!(
        wf.contains("latest.json"),
        "워크플로가 latest.json을 만들지 않습니다."
    );
    // 주석에서는 왜 안 쓰는지 설명하고 있으니 주석은 빼고 본다
    let settings: String = wf
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !settings.contains("includeUpdaterJson"),
        "includeUpdaterJson은 한글 파일 이름을 그대로 가리켜 업데이트가 깨집니다. \
         워크플로에서 latest.json을 직접 만들어야 합니다."
    );
    assert!(
        wf.contains("서명 파일이 없습니다"),
        "서명(.sig)이 없을 때 릴리스를 멈추는 검사가 없습니다."
    );
}

/// 확정된 Release 저장소. 여기가 어긋나면 업데이트 확인이 실패한다.
const REPO: &str = "lohemal/AfterSchool-Total-Manager";

#[test]
fn 저장소_주소가_세_곳에서_같다() {
    // 저장소 주소는 세 곳에 나온다. 하나만 고치고 지나가면 업데이트 확인이
    // 조용히 실패하거나 [Release 페이지 열기]가 없는 곳으로 간다.
    let conf = tauri_conf();
    let expect_base = format!("https://github.com/{REPO}");

    let endpoint = conf["plugins"]["updater"]["endpoints"][0]
        .as_str()
        .expect("updater.endpoints[0]");
    assert_eq!(
        endpoint,
        format!("{expect_base}/releases/latest/download/latest.json"),
        "updater 확인 주소가 확정 저장소({REPO})와 다릅니다."
    );

    assert_eq!(
        conf["bundle"]["homepage"].as_str(),
        Some(expect_base.as_str()),
        "bundle.homepage가 확정 저장소({REPO})와 다릅니다."
    );

    assert_eq!(
        crate::commands::system::RELEASE_URL,
        format!("{expect_base}/releases"),
        "system.rs의 RELEASE_URL이 확정 저장소({REPO})와 다릅니다."
    );
}

#[test]
fn 옛_저장소_이름이_남아_있지_않다() {
    // 임시로 쓰던 이름. 어딘가에 남아 있으면 업데이트가 없는 곳을 본다.
    for (곳, 내용) in [
        ("tauri.conf.json", include_str!("../tauri.conf.json")),
        (
            "commands/system.rs",
            include_str!("commands/system.rs"),
        ),
        (
            "release.yml",
            include_str!("../../.github/workflows/release.yml"),
        ),
    ] {
        assert!(
            !내용.contains("afterschool-manager"),
            "{곳}에 옛 저장소 이름(afterschool-manager)이 남아 있습니다."
        );
    }
}

#[test]
fn 릴리스_첨부_파일_이름이_약속과_같다() {
    // 사용자와 약속한 이름이다. 바뀌면 latest.json의 주소도 함께 어긋난다.
    let wf = include_str!("../../.github/workflows/release.yml");
    assert!(
        wf.contains(r#"$name = "afterschool_${ver}_x64-setup.exe""#),
        "설치파일 이름 규칙(afterschool_<버전>_x64-setup.exe)이 바뀌었습니다."
    );
    for 파일 in ["latest.json", "SHA256SUMS.txt"] {
        assert!(wf.contains(파일), "{파일}을 만들지 않습니다.");
    }
}

#[test]
fn 자료_폴더_식별자가_바뀌지_않았다() {
    // 이 값이 바뀌면 %APPDATA% 아래 폴더가 달라져 기존 자료를 못 찾는다
    let conf = tauri_conf();
    assert_eq!(
        conf["identifier"].as_str(),
        Some("kr.school.afterschool"),
        "identifier를 바꾸면 사용자의 기존 자료를 찾지 못합니다."
    );
}
