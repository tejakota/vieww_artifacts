package dev.three.app;

import android.Manifest;
import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.Context;
import android.content.pm.PackageManager;
import android.graphics.ImageFormat;
import android.hardware.camera2.CameraAccessException;
import android.hardware.camera2.CameraCaptureSession;
import android.hardware.camera2.CameraCharacteristics;
import android.hardware.camera2.CameraDevice;
import android.hardware.camera2.CameraManager;
import android.hardware.camera2.CaptureRequest;
import android.os.Handler;
import android.os.HandlerThread;
import android.util.Range;
import android.view.Surface;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * The camera half of 3's capture pipeline: camera2, polled from Rust.
 *
 * The same rules the vavlt picker shim follows, for the same reasons
 * (android-activity does not surface Java callbacks, and a native method on
 * a class from a foreign classloader would need RegisterNatives):
 *
 * - No Gradle, no AGP: this file is compiled and dexed by three-capture's
 *   build.rs and loaded by Rust through InMemoryDexClassLoader.
 * - Rust polls; Java never calls back. Every poll returns the latest frame
 *   or null, and the queues below drop stale frames so "the latest" is
 *   honest even when the capture loop runs behind the sensor.
 * - The one Java file owns everything camera2 needs: the session, the
 *   readers, the handler thread. Close is idempotent; a stopped shim can be
 *   started again.
 *
 * What it delivers, per poll:
 *
 * - pollColor(): the newest YUV_420_888 frame, repacked by this shim into
 *   contiguous [Y][U][V] planes (row stride == width) so Rust gets one
 *   array and no stride bookkeeping on the Java side of the boundary.
 *   YUV-to-RGB conversion happens in Rust, where it is a tight loop.
 * - pollDepth(): the newest DEPTH16 frame as its raw little-endian shorts
 *   repacked to a contiguous array, plus null when the device has no
 *   DEPTH16 output at all — which is most phones, and the honest answer.
 *   The confidence bits stay packed; Rust filters them.
 * - intrinsics(): CameraCharacteristics.LENS_INTRINSIC_CALIBRATION when the
 *   device publishes it (fx, fy, cx, cy in that order), else null so the
 *   caller falls back to the ~60-degree approximation.
 */
public final class ThreeCameraShim {

    private static final String TAG = "three-camera";

    // ── state, as an int so JNI reads it cheaply ─────────────────────────
    public static final int IDLE = 0;
    public static final int STARTING = 1;
    public static final int RUNNING = 2;
    public static final int STOPPED = 3;
    public static final int PERMISSION_DENIED = 4;
    public static final int ERROR = 5;

    private static final AtomicInteger state = new AtomicInteger(IDLE);

    private static CameraDevice camera;
    private static CameraCaptureSession session;
    private static android.media.ImageReader colorReader;
    private static android.media.ImageReader depthReader;
    private static HandlerThread thread;
    private static Handler handler;
    private static int width;
    private static int height;
    private static int requestedFps = 24;

    /** The latest color frame, repacked. Capacity 2: the newest survives. */
    private static final LinkedBlockingQueue<byte[]> colorQueue = new LinkedBlockingQueue<>(2);
    /** The latest depth frame, repacked. */
    private static final LinkedBlockingQueue<byte[]> depthQueue = new LinkedBlockingQueue<>(2);

    /** The timestamp of the newest color frame, in the camera's clock. */
    private static volatile long lastColorTimestamp;
    private static volatile long lastDepthTimestamp;

    private ThreeCameraShim() {}

    // ── lifecycle ────────────────────────────────────────────────────────

    /** Whether this device's back camera advertises a DEPTH16 output. */
    public static boolean depthSupported(Context context) {
        try {
            String id = backCamera(context);
            if (id == null) return false;
            CameraCharacteristics chars = ((CameraManager) context
                    .getSystemService(Context.CAMERA_SERVICE)).getCameraCharacteristics(id);
            android.util.Size[] depths = chars.get(
                    CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP)
                    .getOutputSizes(ImageFormat.DEPTH16);
            return depths != null && depths.length > 0;
        } catch (CameraAccessException | NullPointerException e) {
            return false;
        }
    }

    /**
     * Open the back camera and start a repeating capture.
     *
     * A call while a session is opening just returns the current state.
     * A call after stop() starts over — the same shim is reusable across
     * captures, which is what a Create tab that records more than one
     * moment needs.
     */
    @SuppressLint("MissingPermission") // checked before the call; see state()
    public static int start(Activity activity, int w, int h, int fps) {
        if (state.get() == RUNNING || state.get() == STARTING) return state.get();

        if (activity.checkSelfPermission(Manifest.permission.CAMERA)
                != PackageManager.PERMISSION_GRANTED) {
            state.set(PERMISSION_DENIED);
            return PERMISSION_DENIED;
        }

        width = w;
        height = h;
        requestedFps = fps;
        state.set(STARTING);
        colorQueue.clear();
        depthQueue.clear();

        try {
            CameraManager manager = (CameraManager) activity.getSystemService(Context.CAMERA_SERVICE);
            String id = backCamera(activity);
            if (id == null) {
                state.set(ERROR);
                return ERROR;
            }

            thread = new HandlerThread("three-camera");
            thread.start();
            handler = new Handler(thread.getLooper());

            colorReader = android.media.ImageReader.newInstance(w, h, ImageFormat.YUV_420_888, 3);
            colorReader.setOnImageAvailableListener(new ColorListener(), handler);

            boolean withDepth = depthSupported(activity);
            if (withDepth) {
                depthReader = android.media.ImageReader.newInstance(w, h, ImageFormat.DEPTH16, 3);
                depthReader.setOnImageAvailableListener(new DepthListener(), handler);
            }

            manager.openCamera(id, new DeviceCallback(), handler);
            return STARTING;
        } catch (CameraAccessException e) {
            android.util.Log.e(TAG, "open camera", e);
            stop();
            state.set(ERROR);
            return ERROR;
        }
    }

    /** The camera opened (or did not): wire the capture session. */
    private static final class DeviceCallback extends CameraDevice.StateCallback {
        @Override public void onOpened(CameraDevice device) {
            camera = device;
            try {
                List<Surface> surfaces = new ArrayList<>();
                surfaces.add(colorReader.getSurface());
                if (depthReader != null) surfaces.add(depthReader.getSurface());
                device.createCaptureSession(surfaces, new SessionCallback(device), handler);
            } catch (CameraAccessException e) {
                android.util.Log.e(TAG, "create session", e);
                state.set(ERROR);
            }
        }
        @Override public void onDisconnected(CameraDevice device) {
            state.set(ERROR);
        }
        @Override public void onError(CameraDevice device, int error) {
            android.util.Log.e(TAG, "camera error " + error);
            state.set(ERROR);
        }
    }

    /** The session configured: start the repeating request. */
    private static final class SessionCallback extends CameraCaptureSession.StateCallback {
        private final CameraDevice device;
        SessionCallback(CameraDevice device) { this.device = device; }
        @Override public void onConfigured(CameraCaptureSession s) {
            try {
                session = s;
                CaptureRequest.Builder builder =
                        device.createCaptureRequest(CameraDevice.TEMPLATE_RECORD);
                builder.addTarget(colorReader.getSurface());
                if (depthReader != null) builder.addTarget(depthReader.getSurface());
                builder.set(CaptureRequest.CONTROL_AE_TARGET_FPS_RANGE,
                        new Range<>(Math.max(1, requestedFps / 2), requestedFps));
                s.setRepeatingRequest(builder.build(), null, handler);
                state.set(RUNNING);
            } catch (CameraAccessException e) {
                android.util.Log.e(TAG, "repeating request", e);
                state.set(ERROR);
            }
        }
        @Override public void onConfigureFailed(CameraCaptureSession s) {
            android.util.Log.e(TAG, "session configure failed");
            state.set(ERROR);
        }
    }

    /** Color frames in: repack, keep the newest. */
    private static final class ColorListener implements android.media.ImageReader.OnImageAvailableListener {
        @Override public void onImageAvailable(android.media.ImageReader reader) {
            android.media.Image image = null;
            try {
                image = reader.acquireLatestImage();
                if (image == null) return;
                byte[] packed = repackYuv(image);
                long ts = image.getTimestamp();
                if (ts != lastColorTimestamp) {
                    lastColorTimestamp = ts;
                    colorQueue.offer(packed);
                    if (colorQueue.size() > 1) colorQueue.poll();
                }
            } finally {
                if (image != null) image.close();
            }
        }
    }

    /** Depth frames in: repack, keep the newest. */
    private static final class DepthListener implements android.media.ImageReader.OnImageAvailableListener {
        @Override public void onImageAvailable(android.media.ImageReader reader) {
            android.media.Image image = null;
            try {
                image = reader.acquireLatestImage();
                if (image == null) return;
                byte[] packed = repackDepth(image);
                long ts = image.getTimestamp();
                if (ts != lastDepthTimestamp) {
                    lastDepthTimestamp = ts;
                    depthQueue.offer(packed);
                    if (depthQueue.size() > 1) depthQueue.poll();
                }
            } finally {
                if (image != null) image.close();
            }
        }
    }

    /** Close everything. Safe to call from any state, from any thread. */
    public static int stop() {
        if (session != null) { try { session.close(); } catch (Throwable t) {} session = null; }
        if (camera != null) { try { camera.close(); } catch (Throwable t) {} camera = null; }
        if (colorReader != null) { try { colorReader.close(); } catch (Throwable t) {} colorReader = null; }
        if (depthReader != null) { try { depthReader.close(); } catch (Throwable t) {} depthReader = null; }
        if (thread != null) { thread.quitSafely(); thread = null; handler = null; }
        colorQueue.clear();
        depthQueue.clear();
        state.set(STOPPED);
        return STOPPED;
    }

    public static int state() { return state.get(); }

    // ── frames ───────────────────────────────────────────────────────────

    /** The newest color frame, repacked contiguous, or null when none is new. */
    public static byte[] pollColor() {
        return colorQueue.poll();
    }

    /** The newest depth frame, repacked contiguous, or null. */
    public static byte[] pollDepth() {
        return depthQueue.poll();
    }

    // ── calibration ──────────────────────────────────────────────────────

    /**
     * LENS_INTRINSIC_CALIBRATION as {fx, fy, cx, cy} in the requested
     * resolution's coordinate system, or null when the device does not
     * publish one — which is common, and the caller's ~60-degree
     * approximation is the fallback.
     *
     * The calibration describes the sensor's active array; the readers run
     * at a scaled size, so the values are scaled here, where the scaling
     * factors are known, rather than making Rust reconstruct them.
     */
    public static float[] intrinsics(Context context) {
        try {
            String id = backCamera(context);
            if (id == null) return null;
            CameraManager manager = (CameraManager) context.getSystemService(Context.CAMERA_SERVICE);
            CameraCharacteristics chars = manager.getCameraCharacteristics(id);
            float[] calibration = chars.get(CameraCharacteristics.LENS_INTRINSIC_CALIBRATION);
            if (calibration == null || calibration.length < 5) return null;
            android.graphics.Rect active = chars.get(CameraCharacteristics.SENSOR_INFO_ACTIVE_ARRAY_SIZE);
            if (active == null || active.width() == 0) return null;
            float sx = width / (float) active.width();
            float sy = height / (float) active.height();
            // LENS_INTRINSIC_CALIBRATION: {fx, fy, cx, cy, s} — with the
            // documented nuance that cx/cy count from the sensor's top-left,
            // which is what the scaled reader also uses.
            return new float[]{calibration[0] * sx, calibration[1] * sy,
                    calibration[2] * sx, calibration[3] * sy};
        } catch (CameraAccessException | NullPointerException e) {
            return null;
        }
    }

    // ── internals ────────────────────────────────────────────────────────

    private static String backCamera(Context context) throws CameraAccessException {
        CameraManager manager = (CameraManager) context.getSystemService(Context.CAMERA_SERVICE);
        String fallback = null;
        for (String id : manager.getCameraIdList()) {
            CameraCharacteristics chars = manager.getCameraCharacteristics(id);
            Integer facing = chars.get(CameraCharacteristics.LENS_FACING);
            if (facing != null && facing == CameraCharacteristics.LENS_FACING_BACK) return id;
            if (fallback == null) fallback = id;
        }
        return fallback;
    }

    /**
     * Repack a YUV_420_888 image into contiguous [Y][U][V] with row stride ==
     * width. The camera's planes carry their own strides and possible
     * interleaving; normalizing here keeps the Rust side a plain triple
     * loop with no stride bookkeeping across the JNI boundary.
     */
    private static byte[] repackYuv(android.media.Image image) {
        android.media.Image.Plane[] planes = image.getPlanes();
        int w = image.getWidth();
        int h = image.getHeight();
        int chromaW = (w + 1) / 2;
        int chromaH = (h + 1) / 2;
        byte[] out = new byte[w * h + chromaW * chromaH * 2];

        copyPlane(planes[0], out, 0, w, h);
        copyPlane(planes[1], out, w * h, chromaW, chromaH);
        copyPlane(planes[2], out, w * h + chromaW * chromaH, chromaW, chromaH);
        return out;
    }

    private static void copyPlane(android.media.Image.Plane plane, byte[] out, int offset,
                                 int w, int h) {
        ByteBuffer buffer = plane.getBuffer();
        int rowStride = plane.getRowStride();
        int pixelStride = plane.getPixelStride();
        int position = buffer.position();
        for (int row = 0; row < h; row++) {
            for (int col = 0; col < w; col++) {
                out[offset + row * w + col] = buffer.get(position + row * rowStride + col * pixelStride);
            }
        }
    }

    /**
     * Repack a DEPTH16 image's shorts contiguously. The confidence bits stay
     * packed exactly as the sensor wrote them: DEPTH16 is a 16-bit sample
     * with depth in millimeters in the low 13 bits and confidence in the top
     * 3, and the filtering decision belongs to Rust where the threshold is
     * one constant next to its tests.
     */
    private static byte[] repackDepth(android.media.Image image) {
        android.media.Image.Plane plane = image.getPlanes()[0];
        int w = image.getWidth();
        int h = image.getHeight();
        ByteBuffer buffer = plane.getBuffer();
        int rowStride = plane.getRowStride();
        // DEPTH16 pixel stride is 2 bytes; the row stride is in bytes.
        int position = buffer.position();
        byte[] out = new byte[w * h * 2];
        for (int row = 0; row < h; row++) {
            int source = position + row * rowStride;
            buffer.position(source);
            buffer.get(out, row * w * 2, w * 2);
        }
        return out;
    }
}
