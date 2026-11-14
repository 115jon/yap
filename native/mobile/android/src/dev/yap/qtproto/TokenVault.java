package dev.yap.qtproto;

import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.Log;

import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.NoSuchFileException;
import java.security.InvalidKeyException;
import java.security.Key;
import java.security.KeyStore;
import java.security.UnrecoverableKeyException;
import java.util.Arrays;

import javax.crypto.AEADBadTagException;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

/**
 * The session token's key: AES-256-GCM in the Android Keystore (made on
 * the device, never exportable). The core seals and opens its session file
 * with it (cpp/keystore.cpp, vault), and the push code opens the same
 * file with the app killed ({@link #readToken}).
 *
 * <p>No user authentication and no unlocked-device requirement: a
 * notification's Reply sends with the app killed and the phone locked, so
 * the key works whenever the app's own files can be read (after the first
 * unlock since boot). What it protects against is a copy of the app's files
 * leaving the device (a backup tool, a rooted phone's file copy, a forensic
 * image): the bytes do not open without this device's key. It does not stop
 * code running as the app on the device, which can use the key too.
 *
 * <p>Sealed layout: version byte, 12-byte IV (the Keystore's own random
 * one), ciphertext and 16-byte tag. The version and a fixed label are the
 * associated data. Every failure returns null and logs the exception's
 * class only, never bytes or messages that could hold them.
 */
public final class TokenVault {
    private TokenVault() {}

    private static final String TAG = "yap-vault";
    private static final String KEYSTORE = "AndroidKeyStore";
    private static final String ALIAS = "yap.session.v1";
    private static final String TRANSFORMATION = "AES/GCM/NoPadding";
    private static final byte VERSION = 1;
    private static final int IV_BYTES = 12;
    private static final int TAG_BITS = 128;
    private static final byte[] LABEL = "yap-session".getBytes(StandardCharsets.UTF_8);

    /** vault SEALED_FILE and LEGACY_FILE. */
    private static final String SEALED_FILE = "session.sealed";
    private static final String LEGACY_FILE = "session.token";

    /** Key creation and deletion, one at a time (a second generate would replace the first key). */
    private static final Object LOCK = new Object();

    /** `plain` sealed with the session key, made first if missing; null on a failure. */
    public static byte[] seal(byte[] plain) {
        try {
            SecretKey key;
            synchronized (LOCK) {
                key = key(true);
            }
            Cipher cipher = Cipher.getInstance(TRANSFORMATION);
            cipher.init(Cipher.ENCRYPT_MODE, key);
            byte[] iv = cipher.getIV();
            if (iv == null || iv.length != IV_BYTES) throw new IllegalStateException("iv");
            cipher.updateAAD(aad());
            byte[] body = cipher.doFinal(plain);
            byte[] out = new byte[1 + IV_BYTES + body.length];
            out[0] = VERSION;
            System.arraycopy(iv, 0, out, 1, IV_BYTES);
            System.arraycopy(body, 0, out, 1 + IV_BYTES, body.length);
            return out;
        } catch (Exception e) {
            Log.w(TAG, "seal failed: " + e.getClass().getSimpleName());
            return null;
        }
    }

    /** Bytes that can never open (see {@link #open}). */
    private static final byte[] INVALID = new byte[0];

    /**
     * What {@link #seal} sealed. An empty array when the bytes can never
     * open (damaged, no key, another key, a key the Keystore invalidated):
     * the core then deletes them and the key, and signs out. Null when the
     * Keystore could not be asked (any other failure): the core keeps the
     * file for the next start.
     */
    public static byte[] open(byte[] sealed) {
        try {
            if (sealed == null || sealed.length < 1 + IV_BYTES + TAG_BITS / 8 || sealed[0] != VERSION) {
                Log.w(TAG, "open: not a sealed session");
                return INVALID;
            }
            SecretKey key;
            synchronized (LOCK) {
                key = key(false);
            }
            if (key == null) {
                Log.w(TAG, "open: no key");
                return INVALID;
            }
            Cipher cipher = Cipher.getInstance(TRANSFORMATION);
            cipher.init(Cipher.DECRYPT_MODE, key, new GCMParameterSpec(TAG_BITS, sealed, 1, IV_BYTES));
            cipher.updateAAD(aad());
            return cipher.doFinal(sealed, 1 + IV_BYTES, sealed.length - 1 - IV_BYTES);
        } catch (AEADBadTagException | InvalidKeyException | UnrecoverableKeyException e) {
            // Another key's bytes, damaged bytes, or a key the Keystore no
            // longer uses (KeyPermanentlyInvalidatedException is an
            // InvalidKeyException).
            Log.w(TAG, "open: cannot open: " + e.getClass().getSimpleName());
            return INVALID;
        } catch (Exception e) {
            Log.w(TAG, "open failed: " + e.getClass().getSimpleName());
            return null;
        }
    }

    /** Drops the key (a sealed session that no longer opens): the next seal makes a new one. */
    public static void reset() {
        synchronized (LOCK) {
            try {
                KeyStore store = KeyStore.getInstance(KEYSTORE);
                store.load(null);
                store.deleteEntry(ALIAS);
            } catch (Exception e) {
                Log.w(TAG, "reset failed: " + e.getClass().getSimpleName());
            }
        }
    }

    /**
     * The session token in the core's data folder, for the push code with
     * the app killed: the sealed file, or before the core's first start on
     * this build the older plain file (the core migrates it). Null when
     * signed out or the sealed file does not open. Never writes: the core
     * owns the files and swaps each in whole, so a read sees one complete
     * version.
     */
    public static String readToken(File dataDir) {
        byte[] sealed;
        try {
            sealed = Files.readAllBytes(new File(dataDir, SEALED_FILE).toPath());
        } catch (NoSuchFileException e) {
            return readLegacy(dataDir);
        } catch (IOException e) {
            Log.w(TAG, "read failed: " + e.getClass().getSimpleName());
            return null;
        }
        byte[] plain = open(sealed);
        if (plain == null || plain.length == 0) return null;
        String token = new String(plain, StandardCharsets.UTF_8).trim();
        Arrays.fill(plain, (byte) 0);
        return token.isEmpty() ? null : token;
    }

    private static String readLegacy(File dataDir) {
        try {
            byte[] bytes = Files.readAllBytes(new File(dataDir, LEGACY_FILE).toPath());
            String token = new String(bytes, StandardCharsets.UTF_8).trim();
            Arrays.fill(bytes, (byte) 0);
            return token.isEmpty() ? null : token;
        } catch (IOException e) {
            return null;
        }
    }

    private static byte[] aad() {
        byte[] aad = new byte[1 + LABEL.length];
        aad[0] = VERSION;
        System.arraycopy(LABEL, 0, aad, 1, LABEL.length);
        return aad;
    }

    /** The key, made when `create` and missing; null when missing. Under LOCK. */
    private static SecretKey key(boolean create) throws Exception {
        KeyStore store = KeyStore.getInstance(KEYSTORE);
        store.load(null);
        // Throws for a key the Keystore can no longer use: the caller fails,
        // and the core resets the key (vault).
        Key existing = store.getKey(ALIAS, null);
        if (existing instanceof SecretKey) return (SecretKey) existing;
        if (!create) return null;
        // Something else under the alias (never written by this app): replaced.
        if (store.containsAlias(ALIAS)) store.deleteEntry(ALIAS);
        KeyGenerator generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE);
        generator.init(new KeyGenParameterSpec.Builder(ALIAS,
                KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                .setKeySize(256)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                // The Keystore picks each IV: never reused by mistake here.
                .setRandomizedEncryptionRequired(true)
                // Background replies with the phone locked (class comment).
                .setUserAuthenticationRequired(false)
                .setUnlockedDeviceRequired(false)
                .build());
        return generator.generateKey();
    }
}
