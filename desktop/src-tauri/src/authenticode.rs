//! Authenticode checks of files this app runs or injects: `WinVerifyTrust`
//! on an open handle, then the primary signer's names, thumbprint, EKUs and
//! timestamp. Used by the OBS-signed capture hook (`game_capture::
//! signed_hook`) and the native app handoff (`native_handoff`), which each
//! apply their own rules to the [`Signer`] this returns.
//!
//! Callers open the file with [`open_read_shared`] (others may read it, but
//! nobody may write, rename or delete it) and keep that handle until the file
//! has been used, so it cannot be swapped between the check and the use.
//!
//! Names are the certificates' CN attributes, not display names (which fall
//! back to other fields when there is no CN). [`verify_signature`] checks no
//! revocation (the capture hook works offline); [`verify_signature_online`]
//! checks the whole chain's revocation on a thread of its own with a
//! deadline, and an unanswered check is a failure, never a pass.

#![cfg(windows)]

use std::fs::File;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `FILE_SHARE_READ`: others may read (and the loader may map the image), but
/// nobody may write, rename, or delete the file while we hold it.
pub const FILE_SHARE_READ: u32 = 0x1;

/// `CRYPT_E_NO_REVOCATION_CHECK`, `CRYPT_E_REVOCATION_OFFLINE`,
/// `CERT_E_REVOCATION_FAILURE`: the revocation servers were not reached.
const REVOCATION_UNKNOWN: [u32; 3] = [0x8009_2012, 0x8009_2013, 0x800B_010E];

/// A file's signer, read from its verified Authenticode signature.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Signer {
    /// The signing certificate's subject CN.
    pub subject: String,
    /// Its issuer's CN.
    pub issuer: String,
    /// The signing certificate's SHA-1 thumbprint, upper-case hex.
    pub thumbprint: String,
    /// Whether the signature carries a valid timestamp countersignature
    /// (RFC 3161 or legacy), which keeps it valid after the certificate
    /// expires.
    pub timestamped: bool,
    /// The signing certificate's enhanced key usage OIDs.
    pub ekus: Vec<String>,
}

/// Why a check did not pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyError {
    pub message: String,
    /// Revocation could not be checked (offline, timed out): no verdict on
    /// the file, but not a pass either.
    pub transient: bool,
}

/// Opens `path` for reading with [`FILE_SHARE_READ`] only.
pub fn open_read_shared(path: &Path) -> Result<File, String> {
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .map_err(|e| format!("open {}: {e}", path.display()))
}

/// `WinVerifyTrust` (Authenticode policy, no UI, no network revocation fetch)
/// on an open file, then the primary signer's subject, issuer, SHA-1
/// thumbprint and EKUs, and whether it is timestamped.
pub fn verify_signature(path: &Path, file: &File) -> Result<Signer, String> {
    verify(path, file, false).map_err(|e| e.message)
}

/// [`verify_signature`] with revocation checked for the whole chain, online.
/// It runs on its own thread with its own read-shared handle (the caller
/// keeps holding theirs, so the bytes cannot change meanwhile). No answer
/// within `deadline`, or an unreachable revocation server, is a transient
/// failure.
pub fn verify_signature_online(path: &Path, deadline: Duration) -> Result<Signer, VerifyError> {
    let (tx, rx) = std::sync::mpsc::channel();
    let owned: PathBuf = path.to_path_buf();
    let spawned = std::thread::Builder::new()
        .name("authenticode-online".into())
        .spawn(move || {
            let result = open_read_shared(&owned)
                .map_err(|message| VerifyError {
                    message,
                    transient: false,
                })
                .and_then(|file| verify(&owned, &file, true));
            let _ = tx.send(result);
        });
    if let Err(e) = spawned {
        return Err(VerifyError {
            message: format!("could not start the signature check: {e}"),
            transient: true,
        });
    }
    rx.recv_timeout(deadline).unwrap_or_else(|_| {
        Err(VerifyError {
            message: format!(
                "the revocation check did not finish in {} s",
                deadline.as_secs()
            ),
            transient: true,
        })
    })
}

fn verify(path: &Path, file: &File, online: bool) -> Result<Signer, VerifyError> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{HANDLE, HWND};
    use windows::Win32::Security::Cryptography::{
        szOID_COMMON_NAME, CertGetCertificateContextProperty, CertGetEnhancedKeyUsage,
        CertGetNameStringW, CERT_CONTEXT, CERT_NAME_ATTR_TYPE, CERT_NAME_ISSUER_FLAG,
        CERT_SHA1_HASH_PROP_ID, CTL_USAGE,
    };
    use windows::Win32::Security::WinTrust::{
        WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain,
        WTHelperProvDataFromStateData, WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2,
        WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL,
        WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_CHAIN, WTD_REVOKE_NONE, WTD_REVOKE_WHOLECHAIN,
        WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    };

    /// The subject's (`flags` 0) or the issuer's (`CERT_NAME_ISSUER_FLAG`)
    /// CN attribute; empty when there is none.
    ///
    /// # Safety
    /// `context` is a valid certificate context.
    unsafe fn cert_cn(context: *const CERT_CONTEXT, flags: u32) -> String {
        let mut name = [0u16; 256];
        // SAFETY: as the caller promises; the buffer is ours.
        let len = unsafe {
            CertGetNameStringW(
                context,
                CERT_NAME_ATTR_TYPE,
                flags,
                Some(szOID_COMMON_NAME.0.cast()),
                Some(&mut name),
            )
        } as usize;
        String::from_utf16_lossy(&name[..len.saturating_sub(1).min(name.len())])
    }

    /// The certificate's EKU OIDs.
    ///
    /// # Safety
    /// `context` is a valid certificate context.
    unsafe fn cert_ekus(context: *const CERT_CONTEXT) -> Vec<String> {
        let mut size = 0u32;
        // SAFETY: as the caller promises; a size query.
        if unsafe { CertGetEnhancedKeyUsage(context, 0, None, &mut size) }.is_err() || size == 0 {
            return Vec::new();
        }
        // u64-aligned storage for the CTL_USAGE and the strings after it.
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let usage = buffer.as_mut_ptr().cast::<CTL_USAGE>();
        // SAFETY: the buffer holds `size` bytes, as the query asked for.
        if unsafe { CertGetEnhancedKeyUsage(context, 0, Some(usage), &mut size) }.is_err() {
            return Vec::new();
        }
        // SAFETY: filled in by the call above; the strings live in `buffer`.
        unsafe {
            let usage = &*usage;
            (0..usage.cUsageIdentifier as usize)
                .filter_map(|i| {
                    let oid = *usage.rgpszUsageIdentifier.add(i);
                    (!oid.is_null()).then(|| String::from_utf8_lossy(oid.as_bytes()).into_owned())
                })
                .collect()
        }
    }

    let fail = |message: String| VerifyError {
        message,
        transient: false,
    };

    let wide_path = HSTRING::from(path.as_os_str());
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide_path.as_ptr()),
        hFile: HANDLE(file.as_raw_handle()),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: if online {
            WTD_REVOKE_WHOLECHAIN
        } else {
            WTD_REVOKE_NONE
        },
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: if online {
            WTD_REVOCATION_CHECK_CHAIN
        } else {
            WTD_CACHE_ONLY_URL_RETRIEVAL
        },
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `data` and everything it points at outlive both calls.
    let status = unsafe {
        WinVerifyTrust(
            HWND(-1isize as *mut _),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };

    let signer = (|| {
        if status != 0 {
            let code = status as u32;
            let transient = REVOCATION_UNKNOWN.contains(&code);
            return Err(VerifyError {
                message: if transient {
                    format!("revocation could not be checked ({code:#010x})")
                } else {
                    format!("Authenticode verification failed ({code:#010x})")
                },
                transient,
            });
        }
        // SAFETY: valid state data from the successful verify above; the
        // returned pointers stay valid until WTD_STATEACTION_CLOSE.
        unsafe {
            let prov = WTHelperProvDataFromStateData(data.hWVTStateData);
            if prov.is_null() {
                return Err(fail("no provider data".into()));
            }
            let sgnr = WTHelperGetProvSignerFromChain(prov, 0, false, 0);
            if sgnr.is_null() {
                return Err(fail("no signer".into()));
            }
            let cert = WTHelperGetProvCertFromChain(sgnr, 0);
            if cert.is_null() || (*cert).pCert.is_null() {
                return Err(fail("no signer certificate".into()));
            }
            let context = (*cert).pCert;
            let subject = cert_cn(context, 0);
            let issuer = cert_cn(context, CERT_NAME_ISSUER_FLAG);
            let mut hash = [0u8; 20];
            let mut hash_len = hash.len() as u32;
            CertGetCertificateContextProperty(
                context,
                CERT_SHA1_HASH_PROP_ID,
                Some(hash.as_mut_ptr().cast()),
                &mut hash_len,
            )
            .map_err(|e| fail(format!("certificate thumbprint: {e}")))?;
            let thumbprint = hash[..hash_len as usize]
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect();
            // WinTrust lists RFC 3161 timestamps among the counter signers
            // too; one that verified has no error of its own.
            let timestamped = (*sgnr).csCounterSigners > 0 && {
                let counter = WTHelperGetProvSignerFromChain(prov, 0, true, 0);
                !counter.is_null() && (*counter).dwError == 0
            };
            Ok(Signer {
                subject,
                issuer,
                thumbprint,
                timestamped,
                ekus: cert_ekus(context),
            })
        }
    })();

    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: closes the state opened above.
    unsafe {
        WinVerifyTrust(
            HWND(-1isize as *mut _),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        );
    }
    signer
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A signed system binary verifies, with names and a timestamp. Skipped
    /// where it is not signed in-file (some images are catalog-signed only).
    #[test]
    fn a_signed_system_file_has_a_signer_and_an_issuer() {
        let Some(windir) = std::env::var_os("WINDIR") else {
            return;
        };
        let path = Path::new(&windir).join(r"explorer.exe");
        let Ok(file) = open_read_shared(&path) else {
            return;
        };
        match verify_signature(&path, &file) {
            Ok(signer) => {
                assert!(!signer.subject.is_empty());
                assert!(!signer.issuer.is_empty());
                assert_eq!(signer.thumbprint.len(), 40);
                assert!(
                    signer.ekus.iter().any(|e| e == "1.3.6.1.5.5.7.3.3"),
                    "{signer:?}"
                );
            }
            Err(err) => eprintln!("skipped: {} is not signed in-file ({err})", path.display()),
        }
    }

    #[test]
    fn an_unsigned_file_fails() {
        let dir = std::env::temp_dir().join(format!("yap-authenticode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("unsigned.exe");
        std::fs::write(&path, b"MZ not really a program").unwrap();
        let file = open_read_shared(&path).unwrap();
        assert!(verify_signature(&path, &file).is_err());
        let online = verify_signature_online(&path, Duration::from_secs(30)).unwrap_err();
        assert!(!online.transient, "{online:?}");
        drop(file);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_held_file_cannot_be_replaced() {
        let dir =
            std::env::temp_dir().join(format!("yap-authenticode-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("held.bin");
        std::fs::write(&path, b"one").unwrap();
        let held = open_read_shared(&path).unwrap();
        assert!(std::fs::write(&path, b"two").is_err(), "write while held");
        assert!(std::fs::remove_file(&path).is_err(), "delete while held");
        assert!(std::fs::read(&path).is_ok(), "others may still read");
        drop(held);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
