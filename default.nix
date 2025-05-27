{ lib
, rustPlatform
, fetchFromGitHub
}:

rustPlatform.buildRustPackage rec {
  pname = "junkanoo";
  version = "0.1.0";

  src = fetchFromGitHub {
    owner = "maschad";
    repo = pname;
    rev = "v${version}";
    sha256 = ""; # You'll need to fill this in after creating the release
  };

  cargoSha256 = ""; # You'll need to fill this in after building

  meta = with lib; {
    description = "Decentralized ephemeral file sharing CLI browser";
    homepage = "https://github.com/maschad/junkanoo";
    license = licenses.mit;
    maintainers = with maintainers; [ ];
  };
}