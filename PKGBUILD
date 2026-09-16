# Maintainer: Ozan Özdil (ozdil) <ozan@pm.me>
pkgname=omarchy-omanotes
pkgver=1.0.0
pkgrel=3
_commit="18d6a4295345fde71bcb1db155572a11f52fb0c4"
pkgdesc="Zero-Knowledge E2EE & Cloud-Sync Google Keep clone for Omarchy Linux"
arch=('x86_64')
url="https://github.com/ozdil/omarchy-omanotes"
license=('MIT')
depends=('glibc' 'quickshell' 'wl-clipboard')
optdepends=(
  'rclone: for Google Drive, WebDAV and Nextcloud synchronization'
  'git: for encrypted Git vault repository synchronization'
)
makedepends=('cargo' 'rust')
source=("$pkgname-$_commit.tar.gz::$url/archive/$_commit.tar.gz")
sha256sums=('cfae117bd74442e7de011e034e849c2cc4250e5b6a9fa03d52c4a16043805372')

build() {
  cd "$pkgname-$_commit"
  cargo build --release --locked
}

package() {
  cd "$pkgname-$_commit"
  install -Dm755 target/release/omanotes-engine "$pkgdir/usr/lib/omarchy/plugins/omanotes/omanotes-engine"
  install -Dm755 omanotes-status "$pkgdir/usr/lib/omarchy/plugins/omanotes/omanotes-status"
  install -Dm755 omanotes-dashboard "$pkgdir/usr/lib/omarchy/plugins/omanotes/omanotes-dashboard"
  install -Dm644 Panel.qml "$pkgdir/usr/lib/omarchy/plugins/omanotes/Panel.qml"
  install -Dm644 manifest.json "$pkgdir/usr/lib/omarchy/plugins/omanotes/manifest.json"
  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
