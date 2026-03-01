# STRIDE Threat Model — MyWallpaper Desktop

## Component-Level STRIDE Analysis

### 1. Remote Frontend (dev.mywallpaper.online)
| Threat | Category | Risk | Mitigation |
|--------|----------|------|------------|
| Server compromise gives full IPC | Tampering/Elevation | HIGH | Minimize IPC permissions, consider bundled frontend |
| XSS via unsafe-inline CSP | Tampering | HIGH | Remove unsafe-inline, use nonces |
| Subdomain takeover on *.mywallpaper.online | Spoofing | MEDIUM | Enumerate explicit subdomains in CSP |
| CDN/DNS hijacking redirects frontend | Spoofing | MEDIUM | SRI hashes, certificate pinning |

### 2. Auto-Updater
| Threat | Category | Risk | Mitigation |
|--------|----------|------|------------|
| Downgrade to older signed version | Tampering | MEDIUM | **FIXED**: Version comparison now rejects downgrades |
| Endpoint override to attacker tag | Tampering | LOW | Endpoint locked to github.com/MyWallpapers/client |
| Signature bypass | Elevation | LOW | minisign + public key in config |

### 3. Deep-Link Handler (mywallpaper://)
| Threat | Category | Risk | Mitigation |
|--------|----------|------|------------|
| Injection payload in URL | Injection | MEDIUM | **FIXED**: Action allowlist + URL normalization |
| Open redirect via deep-link | Spoofing | LOW | Frontend must validate redirects |

### 4. Win32 Desktop Injection
| Threat | Category | Risk | Mitigation |
|--------|----------|------|------------|
| DLL sideloading in user-writable install dir | Elevation | MEDIUM | Directory ACLs, code signing |
| Hook tampering by other processes | Tampering | LOW | Hook runs in our thread, PID validation |
| Undocumented API breaking in Windows update | DoS | MEDIUM | Fallback detection paths exist |

### 5. OAuth Flow
| Threat | Category | Risk | Mitigation |
|--------|----------|------|------------|
| SSRF via private IPs | Spoofing | LOW | **FIXED**: IPv4+IPv6+mapped validation |
| Token interception in browser | Info Disclosure | LOW | Uses system browser, HTTPS enforced |

### 6. IPC Layer
| Threat | Category | Risk | Mitigation |
|--------|----------|------|------------|
| DevTools console IPC abuse | Elevation | MEDIUM | Set devtools:false in production |
| Unauthorized command invocation | Elevation | LOW | Capabilities restrict to listed permissions |

## Risk Matrix (Likelihood x Impact)

| | Low Impact | Medium Impact | High Impact |
|---|---|---|---|
| **High Likelihood** | | M-1 DevTools | H-1 Frontend compromise |
| **Medium Likelihood** | L-1 CSP wildcards | M-2 Deep-link (FIXED) | H-2 CSP frame-src |
| **Low Likelihood** | L-3 Info disclosure | M-4 Downgrade (FIXED) | M-3 Supply chain |

## Top 5 Attack Trees

1. **Remote frontend compromise** -> XSS in WebView -> Call __TAURI__ IPC -> Trigger update/autostart/opener
2. **Deep-link crafting** -> mywallpaper://evil -> (BLOCKED by allowlist) -> Frontend injection
3. **Local privilege escalation** -> Open DevTools -> Execute IPC commands -> Modify system
4. **Update downgrade** -> Override endpoint to old tag -> (BLOCKED by version check) -> Install vuln version
5. **Subdomain takeover** -> *.mywallpaper.online -> CSP bypass -> Script injection

## MITRE ATT&CK Mapping
- T1059.007 (JavaScript execution) - via unsafe-inline CSP
- T1574.001 (DLL search order hijacking) - currentUser install dir
- T1195.002 (Supply chain compromise) - forked wry crate
- T1497 (Virtualization/Sandbox evasion) - undocumented Progman message
