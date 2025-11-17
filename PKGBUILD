pkgname=open-coreui-desktop
pkgver=0.9.6
pkgrel=1
pkgdesc="Open CoreUI Desktop Application - A lightweight implementation of Open WebUI"
arch=('x86_64' 'aarch64')
url="https://github.com/xxnuo/open-coreui"
license=('LICENSE')
depends=('cairo' 'desktop-file-utils' 'gdk-pixbuf2' 'glib2' 'gtk3' 'hicolor-icon-theme' 'libsoup' 'pango' 'webkit2gtk-4.1')
makedepends=()
options=('!strip' '!emptydirs')
install=${pkgname}.install
source_x86_64=("${url}/releases/download/v${pkgver}/Open.CoreUI.Desktop_${pkgver}_amd64.deb")
source_aarch64=("${url}/releases/download/v${pkgver}/Open.CoreUI.Desktop_${pkgver}_arm64.deb")
sha256sums_x86_64=('SKIP')
sha256sums_aarch64=('SKIP')

package() {
  # Extract deb package
  tar -xf data.tar.gz -C "${pkgdir}"
}
