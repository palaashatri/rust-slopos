# SLOPOS-I Package Repository Enrollment

SLOPOS-I publishes official signed package repositories for Debian/Ubuntu-family and Arch Linux distributions.

## Debian / Ubuntu / Mint Enrollment

To enroll your system in the SLOPOS-I package repository:

```bash
# 1. Download and install the repository signing key
sudo mkdir -p /etc/apt/keyrings
curl -fsSL https://repo.slopos.org/keys/slopos-archive-keyring.gpg \
  | sudo tee /etc/apt/keyrings/slopos-archive-keyring.gpg > /dev/null

# 2. Add the SLOPOS-I repository source
echo "deb [signed-by=/etc/apt/keyrings/slopos-archive-keyring.gpg] https://repo.slopos.org/apt alpha main" \
  | sudo tee /etc/apt/sources.list.d/slopos.list

# 3. Update package index and install SLOPOS-I
sudo apt update
sudo apt install slopos-i
```

To update SLOPOS-I in the future:
```bash
sudo apt update && sudo apt upgrade slopos-i
```

---

## Arch Linux / Manjaro Enrollment

To enroll your system in the SLOPOS-I pacman repository:

```bash
# 1. Import and sign the SLOPOS repository key
sudo pacman-key --recv-keys 4A8F90C12E345678 --keyserver keyserver.ubuntu.com
sudo pacman-key --lsign-key 4A8F90C12E345678

# 2. Add the repository to /etc/pacman.conf
sudo tee -a /etc/pacman.conf <<'EOF'

[slopos]
SigLevel = Required DatabaseOptional
Server = https://repo.slopos.org/pacman
EOF

# 3. Synchronize repository databases and install SLOPOS-I
sudo pacman -Sy slopos-i
```

To update SLOPOS-I in the future:
```bash
sudo pacman -Syu
```
