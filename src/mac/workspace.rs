//! NSWorkspace helpers: frontmost app, app icons, activation, Finder/URL opening.
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplicationActivationOptions, NSBitmapImageFileType, NSBitmapImageRep, NSImage, NSRunningApplication,
    NSWorkspace,
};
use objc2_foundation::{NSArray, NSBundle, NSDictionary, NSString, NSURL};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct FrontApp {
    pub pid: i32,
    pub bundle_id: String,
    pub name: String,
    pub path: Option<PathBuf>,
}

fn describe(app: &NSRunningApplication) -> FrontApp {
    FrontApp {
        pid: app.processIdentifier(),
        bundle_id: app.bundleIdentifier().map(|s| s.to_string()).unwrap_or_default(),
        name: app.localizedName().map(|s| s.to_string()).unwrap_or_default(),
        path: app.bundleURL().and_then(|u| u.path()).map(|p| PathBuf::from(p.to_string())),
    }
}

pub fn frontmost_app() -> Option<FrontApp> {
    let ws = NSWorkspace::sharedWorkspace();
    ws.frontmostApplication().map(|a| describe(&a))
}

pub fn activate_pid(pid: i32) -> bool {
    match NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
        #[allow(deprecated)]
        Some(app) => app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps),
        None => false,
    }
}

pub fn is_pid_active(pid: i32) -> bool {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .map(|a| a.isActive())
        .unwrap_or(false)
}

/// Converts any image data AppKit can read (TIFF, HEIC, JPEG, …) to PNG bytes.
pub fn data_to_png(data: &[u8]) -> Option<Vec<u8>> {
    objc2::rc::autoreleasepool(|_| {
        let ns = objc2_foundation::NSData::with_bytes(data);
        let rep = NSBitmapImageRep::imageRepWithData(&ns)?;
        let props: Retained<NSDictionary<_, _>> = NSDictionary::new();
        let out = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &props) }?;
        Some(out.to_vec())
    })
}

fn image_to_png(image: &NSImage, target_px: usize) -> Option<Vec<u8>> {
    objc2::rc::autoreleasepool(|_| image_to_png_inner(image, target_px))
}

fn image_to_png_inner(image: &NSImage, target_px: usize) -> Option<Vec<u8>> {
    let tiff = image.TIFFRepresentation()?;
    let reps = NSBitmapImageRep::imageRepsWithData(&tiff);
    let mut best: Option<Retained<NSBitmapImageRep>> = None;
    let mut best_w = 0isize;
    for rep in reps.iter() {
        let Ok(bmp) = rep.downcast::<NSBitmapImageRep>() else { continue };
        let w = bmp.pixelsWide();
        let better = match best_w {
            0 => true,
            bw if bw < target_px as isize => w > bw,
            bw => w >= target_px as isize && w < bw,
        };
        if better {
            best_w = w;
            best = Some(bmp);
        }
    }
    let bmp = best?;
    let props: Retained<NSDictionary<_, _>> = NSDictionary::new();
    let data = unsafe { bmp.representationUsingType_properties(NSBitmapImageFileType::PNG, &props) }?;
    Some(data.to_vec())
}

/// Renders the icon for an application bundle (or any file) to PNG bytes.
pub fn icon_png_for_path(path: &Path, target_px: usize) -> Option<Vec<u8>> {
    let ws = NSWorkspace::sharedWorkspace();
    let image = ws.iconForFile(&NSString::from_str(&path.to_string_lossy()));
    image_to_png(&image, target_px)
}

/// Returns the bundle identifier of an application bundle at `path`.
pub fn bundle_id_for_app(path: &Path) -> Option<String> {
    let bundle = NSBundle::bundleWithPath(&NSString::from_str(&path.to_string_lossy()))?;
    bundle.bundleIdentifier().map(|s| s.to_string())
}

pub fn app_display_name(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

pub fn reveal_in_finder(paths: &[PathBuf]) {
    let ws = NSWorkspace::sharedWorkspace();
    let urls: Vec<Retained<NSURL>> = paths
        .iter()
        .map(|p| NSURL::fileURLWithPath(&NSString::from_str(&p.to_string_lossy())))
        .collect();
    let arr = NSArray::from_retained_slice(&urls);
    ws.activateFileViewerSelectingURLs(&arr);
}

pub fn open_url(url: &str) -> bool {
    let ws = NSWorkspace::sharedWorkspace();
    match NSURL::URLWithString(&NSString::from_str(url)) {
        Some(u) => ws.openURL(&u),
        None => false,
    }
}

