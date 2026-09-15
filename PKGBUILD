# Maintainer: Ozan Özdil (ozdil) <ozan@pm.me>
pkgname=omarchy-omanotes
pkgver=1.0.0
pkgrel=2
_commit="47b2640eeb24884d1a553636230e7f245bbdf496"
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
sha256sums=('7fade8cdb0d7258b88ea3bb16f6f14f8d8f6d32798b0ff70d8c5f714476b2374')

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
