# Maintainer: Your Name <your.email@example.com>
pkgname=junkanoo
pkgver=0.1.0
pkgrel=1
pkgdesc="Decentralized ephemeral file sharing CLI browser"
arch=('x86_64')
url="https://github.com/maschad/junkanoo"
license=('MIT')
makedepends=('rust' 'cargo')
source=("$pkgname-$pkgver.tar.gz::https://github.com/maschad/junkanoo/archive/v$pkgver.tar.gz")
sha256sums=('') # You'll need to fill this in after creating the release

build() {
  cd "$pkgname-$pkgver"
  cargo build --release --locked
}

package() {
  cd "$pkgname-$pkgver"
  install -Dm755 "target/release/junkanoo" "$pkgdir/usr/bin/junkanoo"
  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}