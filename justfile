obs_app    := "/Applications/OBS.app"
libobs_dir := obs_app + "/Contents/Frameworks/libobs.framework"
vendor_lib := justfile_directory() + "/vendor/lib"
plugin_dir := env_var("HOME") + "/Library/Application Support/obs-studio/plugins/obs-ntsc.plugin"

# Default: list recipes
default:
    @just --list

# Create a vendor/lib dir with a symlink named libobs.0.dylib pointing at the
# framework binary, so obs-sys (which expects the legacy flat dylib name) can
# satisfy its `-lobs.0` link directive against the installed OBS.app.
setup-libobs:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ ! -f "{{libobs_dir}}/libobs" ]]; then
        echo "libobs not found at {{libobs_dir}}/libobs — is OBS.app installed?" >&2
        exit 1
    fi
    mkdir -p "{{vendor_lib}}"
    ln -sf "{{libobs_dir}}/libobs" "{{vendor_lib}}/libobs.0.dylib"

# Build release dylib. Requires `just setup-libobs` to have been run once.
build: setup-libobs
    LIBOBS_PATH="{{vendor_lib}}" cargo build --release

# Install as an OBS .plugin bundle at the user plugin path.
install: build
    #!/usr/bin/env bash
    set -euo pipefail
    BUNDLE="{{plugin_dir}}"
    rm -rf "$BUNDLE"
    mkdir -p "$BUNDLE/Contents/MacOS"
    cp target/release/libobs_ntsc.dylib "$BUNDLE/Contents/MacOS/obs-ntsc"
    # Rewrite the install_name from the build path so OBS doesn't get confused
    # if any sub-link uses LC_ID_DYLIB. Bundle plugins normally have none.
    install_name_tool -id "@rpath/obs-ntsc" "$BUNDLE/Contents/MacOS/obs-ntsc"
    cat > "$BUNDLE/Contents/Info.plist" <<'PLIST'
    <?xml version="1.0" encoding="UTF-8"?>
    <!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
    <plist version="1.0">
    <dict>
        <key>CFBundleName</key><string>obs-ntsc</string>
        <key>CFBundleIdentifier</key><string>com.adamisrael.obs-ntsc</string>
        <key>CFBundleVersion</key><string>0.1.0</string>
        <key>CFBundleShortVersionString</key><string>0.1.0</string>
        <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
        <key>CFBundleExecutable</key><string>obs-ntsc</string>
        <key>CFBundlePackageType</key><string>BNDL</string>
        <key>CFBundleSupportedPlatforms</key><array><string>MacOSX</string></array>
        <key>LSMinimumSystemVersion</key><string>11.0</string>
    </dict>
    </plist>
    PLIST
    echo "Installed to $BUNDLE"

clean:
    cargo clean
    rm -rf vendor
