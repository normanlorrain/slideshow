# Linux tips

## Helpful packages

```
sudo apt update
sudo apt upgrade
sudo apt install openssh-server samba
# for building magic-lantern: libsdl2-dev fonts-freefont-ttf (see docs/build_rust.md)
```
## Ubuntu/Gnome

***~/.config/autostart/magic-lantern.desktop***

```ini
[Desktop Entry]
Type=Application
Exec=magic-lantern /home/norman/country-flags/png1000px/
Hidden=false
NoDisplay=false
X-GNOME-Autostart-enabled=true
Name[en_US]=magic-lantern
Name=magic-lantern
Comment[en_US]=testing
Comment=testing
```

## Ubuntu/LxQt

***.config/autostart/magic-lantern.desktop***

```ini
[Desktop Entry]
Exec=magic-lantern -c /home/norman/magic-lantern.toml
Name=magic-lantern
Type=Application
Version=1.0
X-LXQt-Need-Tray=true
```
