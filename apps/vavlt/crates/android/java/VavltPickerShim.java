package dev.vavlt.app;

import android.app.Activity;
import android.app.Fragment;
import android.content.ClipData;
import android.content.ContentResolver;
import android.content.Intent;
import android.util.Log;
import android.database.Cursor;
import android.graphics.Bitmap;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.ParcelFileDescriptor;
import android.provider.MediaStore;
import android.provider.OpenableColumns;
import android.util.Size;

import java.io.ByteArrayOutputStream;
import java.util.ArrayList;

/**
 * The shim SPEC §6 says is unavoidable: android-activity does not surface
 * onActivityResult (rust-mobile/android-activity#174), JNI can call Java methods
 * but cannot define them, and NativeActivity extends plain Activity so
 * registerForActivityResult is out of reach.
 *
 * A Fragment is the way out that keeps this file the only Java in the repo: the
 * framework delivers onActivityResult to fragments, and fragments are attached
 * at runtime, so this class needs no manifest entry — which means no Gradle, no
 * AGP, no Kotlin. It is compiled to a dex by build.rs and loaded by Rust through
 * InMemoryDexClassLoader, exactly as Slint loads its own helper.
 *
 * Rust polls {@link #state()} rather than being called back: a native method on
 * a class loaded by a *different* classloader than the one that loaded the .so
 * would need explicit RegisterNatives, and polling a static costs one JNI call
 * every 120ms only while a pick is actually in flight.
 *
 * This returns metadata only — URI, display name, size. No file is opened here.
 * Opening is {@link #openFd} and happens after the user grants Transform, so the
 * scan screen genuinely reads no pixels (SPEC §1.1: the fd is the capability).
 */
public class VavltPickerShim extends Fragment {

    private static final String TAG = "vavlt-picker";

    public static final int IDLE = 0;
    public static final int PENDING = 1;
    public static final int DONE = 2;
    public static final int CANCELLED = 3;

    private static final int REQ_PICK = 0x5641; // 'VA'

    private static volatile int sState = IDLE;
    private static volatile String[] sUris = new String[0];
    private static volatile String[] sNames = new String[0];
    private static volatile long[] sSizes = new long[0];

    private Intent pending;

    // --- called from Rust -------------------------------------------------

    /**
     * Opens the system picker. Out-of-process; grants nothing ambient.
     *
     * Rust calls this from the native main thread, which is not the Android
     * main thread — NativeActivity runs android_main on its own. Fragment
     * transactions and startActivityForResult both belong to the UI thread, so
     * the hop happens here rather than being a rule the Rust side has to know.
     */
    public static void launch(final Activity activity, final int max) {
        sState = PENDING;
        activity.runOnUiThread(new Runnable() {
            @Override public void run() { launchOnUiThread(activity, max); }
        });
    }

    private static void launchOnUiThread(Activity activity, int max) {
        sUris = new String[0];
        sNames = new String[0];
        sSizes = new long[0];
        // PENDING is already set by launch(), before the hop to the UI thread,
        // so a poll that lands between the two sees a pick in flight rather
        // than an idle picker.

        Intent intent;
        if (Build.VERSION.SDK_INT >= 33) {
            // Android Photo Picker: no permission, and the user sees only what
            // they hand over.
            intent = new Intent(MediaStore.ACTION_PICK_IMAGES);

            // CLAMPED, and this is not defensive tidying — it is the bug.
            //
            // EXTRA_PICK_IMAGES_MAX must be <= MediaStore.getPickImagesMaxLimit(),
            // which is 100 on essentially every device. Passing more does not
            // clamp and does not warn: the picker activity throws
            // IllegalArgumentException as it starts, so the user taps "Choose
            // photos" and nothing opens. We were passing MAX_PICK, which is 200.
            //
            // The limit is read rather than hardcoded because it is a device
            // property and has already changed once.
            int cap = 100;
            try {
                cap = MediaStore.getPickImagesMaxLimit();
            } catch (Throwable ignored) {
                // Older middleware without the accessor. 100 is the documented
                // floor and is safe everywhere the photo picker exists at all.
            }
            int wanted = Math.max(2, Math.min(max, cap));
            intent.putExtra(MediaStore.EXTRA_PICK_IMAGES_MAX, wanted);
            Log.i(TAG, "photo picker: asking for up to " + wanted + " (device cap " + cap + ")");
        } else {
            // API 30–32 predate the photo picker. SAF is the equivalent
            // out-of-process picker and is likewise permission-free.
            intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType("image/*");
            intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
        }

        VavltPickerShim f = new VavltPickerShim();
        f.pending = intent;
        try {
            activity.getFragmentManager()
                    .beginTransaction()
                    .add(f, "vavlt-picker")
                    .commitAllowingStateLoss();
            activity.getFragmentManager().executePendingTransactions();
        } catch (Throwable t) {
            // A throw here used to leave sState at IDLE for ever, so the Rust
            // side waited out its ten-minute patience on a picker that was
            // never going to appear. Report it as a cancel and say why.
            Log.e(TAG, "could not attach the picker fragment", t);
            sState = CANCELLED;
        }
    }

    public static int state() { return sState; }
    public static String[] uris() { return sUris; }
    public static String[] names() { return sNames; }
    public static long[] sizes() { return sSizes; }

    public static void reset() {
        sState = IDLE;
        sUris = new String[0];
        sNames = new String[0];
        sSizes = new long[0];
    }

    /**
     * A display thumbnail for one picked item, as JPEG bytes.
     *
     * This is the system's own downscaled render (`loadThumbnail`, API 29+),
     * the same image the picker just showed the user — not a decode of the
     * original, and not something the engine ever sees. It exists so the plan
     * screen can show you the photographs you are about to change instead of a
     * list of filenames. The engine's read path is still {@link #openFd} and
     * still happens only after the Transform grant.
     *
     * Returns null rather than throwing: a missing thumbnail costs a tile, not
     * a run.
     */
    public static byte[] thumbnail(Activity activity, String uri, int size) {
        try {
            Bitmap bm = activity.getContentResolver()
                    .loadThumbnail(Uri.parse(uri), new Size(size, size), null);
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            bm.compress(Bitmap.CompressFormat.JPEG, 82, out);
            bm.recycle();
            return out.toByteArray();
        } catch (Throwable t) {
            return null;
        }
    }

    /**
     * Detaches a read-only fd for one URI. The caller in Rust owns it and must
     * close it. Returns -1 if the grant has lapsed or the URI is not readable.
     */
    public static int openFd(Activity activity, String uri) {
        try {
            ParcelFileDescriptor pfd = activity.getContentResolver()
                    .openFileDescriptor(Uri.parse(uri), "r");
            if (pfd == null) return -1;
            return pfd.detachFd();
        } catch (Exception e) {
            return -1;
        }
    }

    // --- fragment lifecycle -----------------------------------------------

    @Override
    public void onCreate(Bundle saved) {
        super.onCreate(saved);
        if (pending != null) {
            try {
                startActivityForResult(pending, REQ_PICK);
            } catch (Throwable t) {
                // The IllegalArgumentException from an over-cap
                // EXTRA_PICK_IMAGES_MAX lands here, and used to vanish: the
                // fragment stayed attached, no result ever arrived, and the app
                // showed a button that did nothing.
                Log.e(TAG, "the picker refused to start", t);
                sState = CANCELLED;
                detach();
            }
            pending = null;
        }
    }

    @Override
    public void onActivityResult(int req, int res, Intent data) {
        super.onActivityResult(req, res, data);
        if (req != REQ_PICK) return;

        if (res != Activity.RESULT_OK || data == null) {
            sState = CANCELLED;
            detach();
            return;
        }

        ArrayList<Uri> picked = new ArrayList<>();
        ClipData clip = data.getClipData();
        if (clip != null) {
            for (int i = 0; i < clip.getItemCount(); i++) {
                picked.add(clip.getItemAt(i).getUri());
            }
        } else if (data.getData() != null) {
            picked.add(data.getData());
        }

        int n = picked.size();
        String[] uris = new String[n];
        String[] names = new String[n];
        long[] sizes = new long[n];
        ContentResolver cr = getActivity().getContentResolver();

        for (int i = 0; i < n; i++) {
            Uri u = picked.get(i);
            uris[i] = u.toString();
            names[i] = u.getLastPathSegment();
            sizes[i] = -1;
            // OpenableColumns is metadata the picker already surfaced to the
            // user; querying it opens no file.
            try (Cursor c = cr.query(u, null, null, null, null)) {
                if (c != null && c.moveToFirst()) {
                    int ni = c.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                    int si = c.getColumnIndex(OpenableColumns.SIZE);
                    if (ni >= 0 && !c.isNull(ni)) names[i] = c.getString(ni);
                    if (si >= 0 && !c.isNull(si)) sizes[i] = c.getLong(si);
                }
            } catch (Exception ignored) {
            }
        }

        sUris = uris;
        sNames = names;
        sSizes = sizes;
        sState = DONE;
        detach();
    }

    private void detach() {
        try {
            getFragmentManager().beginTransaction().remove(this).commitAllowingStateLoss();
        } catch (Exception ignored) {
        }
    }
}
