# Maintainer: Ozan Özdil (ozdil) <ozan@pm.me>
pkgname=omarchy-omanotes
pkgver=1.4.0
pkgrel=1
pkgdesc="Zero-Knowledge E2EE & Cloud-Sync Google Keep clone for Omarchy Linux"
arch=('x86_64')
url="https://github.com/ozdil/omarchy-omanotes"
license=('MIT')
depends=('glibc' 'gcc-libs' 'quickshell' 'wl-clipboard')
optdepends=(
  'rclone: for Google Drive, WebDAV and Nextcloud synchronization'
  'git: for encrypted Git vault repository synchronization'
)
makedepends=('cargo' 'rust')
source=()
sha256sums=()

_rust_host() {
  rustc -vV | sed -n 's/host: //p'
}

_bin() {
  echo "${srcdir}/cargo-target/$(_rust_host)/release/omanotes-engine"
}

prepare() {
  cd "${startdir}"
  cargo fetch --locked --target "$(_rust_host)"
}

build() {
  cd "${startdir}"
  cargo build --release --frozen --target "$(_rust_host)" --target-dir "${srcdir}/cargo-target"
}

check() {
  cd "${startdir}"
  cargo test --release --frozen --target "$(_rust_host)" --target-dir "${srcdir}/cargo-target"
}

package() {
  cd "${startdir}"
  local bin source_hash bin_hash
  bin="$(_bin)"
  source_hash="$(/usr/bin/find src Cargo.toml Cargo.lock -type f | /usr/bin/sort | /usr/bin/xargs /usr/bin/sha256sum | /usr/bin/sha256sum | /usr/bin/awk '{print $1}')"
  bin_hash="$(/usr/bin/sha256sum "${bin}" | /usr/bin/awk '{print $1}')"

  install -Dm755 "${bin}" "${pkgdir}/usr/bin/omanotes-engine"
  install -Dm644 "omanotes.desktop" "${pkgdir}/usr/share/applications/omanotes.desktop"

  for plugdir in "${pkgdir}/usr/lib/omarchy/plugins/omanotes" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omanotes"; do
    install -Dm755 "${bin}" "${plugdir}/omanotes-engine"
    install -Dm755 "omanotes-status" "${plugdir}/omanotes-status"
    install -Dm755 "omanotes-dashboard" "${plugdir}/omanotes-dashboard"
    install -Dm644 "Panel.qml" "${plugdir}/Panel.qml"
    install -Dm644 "manifest.json" "${plugdir}/manifest.json"
    install -Dm644 "qml/BoundedTextInput.qml" "${plugdir}/qml/BoundedTextInput.qml"
    install -Dm644 "qml/BoundedPasswordInput.qml" "${plugdir}/qml/BoundedPasswordInput.qml"
    install -Dm644 "qml/MainWindow.qml" "${plugdir}/qml/MainWindow.qml"
    install -Dm644 "qml/shell.qml" "${plugdir}/qml/shell.qml"
    install -Dm644 "qml/theme/Theme.qml" "${plugdir}/qml/theme/Theme.qml"
    install -Dm644 "qml/theme/qmldir" "${plugdir}/qml/theme/qmldir"
    printf '%s %s\n' "${source_hash}" "${bin_hash}" > "${plugdir}/.engine-provenance"
    chmod 644 "${plugdir}/.engine-provenance"
  done

  install -Dm644 "README.md" "${pkgdir}/usr/share/doc/${pkgname}/README.md"
  install -Dm644 "LICENSE" "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE"
}
