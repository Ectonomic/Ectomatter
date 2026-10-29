# Ectomatter as an RPM, made by tools/package-rpm.sh from the tree
# tools/install.sh stages under /usr (--define "staged DIR"), at the app's
# own version (--define "version X"). Every file comes from that tree; the
# libraries the programs need are found by rpmbuild from the programs.
Name:           ectomatter
Version:        %{version}
Release:        1%{?dist}
Summary:        Lighting, keys and screens of keyboards and mice, set from one app
License:        GPL-3.0-only
URL:            https://github.com/Ectonomic/Ectomatter
# (the libraries the programs link are found by rpmbuild; the window opens
# Wayland's itself, as it starts)
Requires:       libwayland-client.so.0()(64bit)

# (built before rpmbuild runs, so there is nothing to build here, and no
# debug symbols to split off)
%global debug_package %{nil}

%description
Ectomatter sets up the keyboards and mice it has a core for: their lights,
keys and screens. Plugins bring in other programs, such as OpenRGB, the
games on the computer, its sensors and the weather.

%install
cp -a %{staged}/. %{buildroot}/

%files
%{_bindir}/ectomatter
%{_bindir}/ectomatter-launch
%{_datadir}/ectomatter/
%{_datadir}/applications/com.ectonomic.ectomatter.desktop
%{_datadir}/icons/hicolor/scalable/apps/com.ectonomic.ectomatter.svg
%{_udevrulesdir}/70-ectomatter.rules
%doc %{_datadir}/doc/ectomatter/
%license %{_datadir}/licenses/ectomatter/

%post
# (the rules let the person at the computer open the devices; udev reads
# them again itself where the system does that for every package)
%udev_rules_update
