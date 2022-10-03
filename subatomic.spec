%define debug_package %{nil}
%define _build_id_links none
Name:           subatomic
Version:        0.1.0
Release:        5%{?dist}
Summary:        A compose tool

License:        MIT
URL:            https://github.com/FyraLabs/subatomic
Source0:        https://github.com/FyraLabs/subatomic/archive/refs/heads/main.zip

BuildRequires:  go-rpm-macros
BuildRequires:  git-core
BuildRequires:  ostree-devel
Requires:       ostree
Requires:       createrepo_c

%description
A compose tool


%package cli
Summary:        Client for Subatomic repo manager

%description cli
Client for Subatomic repo manager

%files cli
%{_bindir}/subatomic-cli

%prep
%autosetup -n subatomic-main


%build

mkdir -p build/bin
go build -v -o build/bin/subatomic-cli ./subatomic-cli
go build -v -o build/bin/subatomic ./server


%install
mkdir -p %{buildroot}%{_bindir}/
install -pm 755 build/bin/subatomic-cli %{buildroot}%{_bindir}/
install -pm 755 build/bin/subatomic %{buildroot}%{_bindir}/


%files
%{_bindir}/subatomic



%changelog
* Fri Sep 30 2022 Cappy Ishihara <cappy@cappuchino.xyz>
- Intial release
