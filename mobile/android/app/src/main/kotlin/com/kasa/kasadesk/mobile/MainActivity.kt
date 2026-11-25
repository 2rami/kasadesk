package com.kasa.kasadesk.mobile

import android.media.MediaCodec
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.view.Surface
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import io.flutter.view.TextureRegistry
import java.nio.ByteBuffer

class MainActivity : FlutterActivity() {
    private val decoders = mutableMapOf<Long, ScreenDecoder>()
    private var channel: MethodChannel? = null

    override fun configureFlutterEngine(engine: FlutterEngine) {
        super.configureFlutterEngine(engine)
        channel = MethodChannel(engine.dartExecutor.binaryMessenger, "kasadesk/android_video").also { api ->
            api.setMethodCallHandler { call, result ->
                when (call.method) {
                    "create" -> {
                        val hevc = MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.any {
                            !it.isEncoder && it.supportedTypes.any { mime -> mime.equals("video/hevc", true) }
                        }
                        if (hevc) {
                            val texture = engine.renderer.createSurfaceProducer()
                            decoders[texture.id()] = ScreenDecoder(texture)
                            result.success(texture.id())
                        } else result.success(null)
                    }
                    "frame" -> {
                        val decoder = call.argument<Number>("id")?.toLong()?.let(decoders::get)
                        val bytes = call.argument<ByteArray>("bytes")
                        if (decoder == null || bytes == null) result.success(false)
                        else decoder.frame(bytes) { result.success(it) }
                    }
                    "reset", "dispose" -> {
                        val id = (call.arguments as? Number)?.toLong()
                        if (call.method == "reset") decoders[id]?.reset() else decoders.remove(id)?.close()
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            }
        }
    }

    override fun cleanUpFlutterEngine(engine: FlutterEngine) {
        channel?.setMethodCallHandler(null)
        decoders.values.forEach { it.close() }
        decoders.clear()
        super.cleanUpFlutterEngine(engine)
    }
}

private class ScreenDecoder(private val texture: TextureRegistry.SurfaceProducer) {
    private val worker = HandlerThread("KasaDesk-video").apply { start() }
    private val handler = Handler(worker.looper)
    private val main = Handler(Looper.getMainLooper())
    private var size = 0L
    private var codec: MediaCodec? = null
    private var config = byteArrayOf()
    private var width = 0
    private var height = 0
    private var ready = false

    fun frame(bytes: ByteArray, done: (Boolean) -> Unit) {
        if (bytes.size < 40) { done(false); return }
        val head = ByteBuffer.wrap(bytes)
        val w = head.getInt(30)
        val h = head.getInt(34)
        if (w !in 1..8192 || h !in 1..8192 || w.toLong() * h > 33_554_432) { done(false); return }
        val dimensions = (w.toLong() shl 32) or h.toLong()
        if (dimensions != size) { texture.setSize(w, h); size = dimensions }
        val surface = texture.surface
        handler.post {
            val ok = try { decode(bytes, surface) } catch (_: Exception) { stop(); false }
            main.post { done(ok) }
        }
    }

    private fun decode(bytes: ByteArray, surface: Surface): Boolean {
        if (bytes.size !in 40..(16 * 1024 * 1024) || bytes[0] != 0x46.toByte()) return false
        val input = ByteBuffer.wrap(bytes)
        val key = bytes[13] != 0.toByte()
        if (!ready && !key) return false
        input.position(30)
        val w = input.int
        val h = input.int
        if (w !in 1..8192 || h !in 1..8192 || w.toLong() * h > 33_554_432) return false
        val count = input.get().toInt() and 255
        val sets = ArrayList<ByteArray>(count)
        repeat(count) {
            val size = input.short.toInt() and 65535
            require(size > 0 && size <= input.remaining())
            sets.add(ByteArray(size).also { input.get(it) })
        }
        if (key && sets.isNotEmpty()) {
            val header = ByteBuffer.allocate(sets.sumOf { it.size + 4 })
            sets.forEach { header.putInt(1).put(it) }
            if (codec == null || w != width || h != height || !config.contentEquals(header.array())) {
                stop()
                val name = if ((sets.first()[0].toInt() and 31) == 7) "video/avc" else "video/hevc"
                val format = MediaFormat.createVideoFormat(name, w, h)
                if (android.os.Build.VERSION.SDK_INT >= 30) format.setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                format.setByteBuffer("csd-0", ByteBuffer.wrap(header.array()))
                codec = MediaCodec.createDecoderByType(name)
                codec!!.configure(format, surface, null, 0)
                codec!!.start()
                width = w; height = h; config = header.array()
            }
        }
        val c = codec ?: return false
        drain(c)
        val slot = c.dequeueInputBuffer(5000)
        if (slot < 0) { ready = false; return false }
        val dst = c.getInputBuffer(slot) ?: error("buffer")
        dst.clear()
        dst.order(java.nio.ByteOrder.BIG_ENDIAN)
        while (input.hasRemaining()) {
            val n = input.int
            require(n > 0 && n <= input.remaining() && dst.remaining() >= n + 4)
            dst.putInt(1).put(bytes, input.position(), n)
            input.position(input.position() + n)
        }
        c.queueInputBuffer(slot, 0, dst.position(), System.nanoTime()/1000, 0)
        ready = true
        val rendered = drain(c)
        // Even a static desktop must present the final decoded image.
        handler.removeCallbacks(present)
        if (!rendered) { remainingDrains = 16; handler.postDelayed(present, 12) }
        return true
    }

    private var remainingDrains = 0
    private val present = object : Runnable {
        override fun run() {
            try {
                val c = codec ?: return
                if (!drain(c, 8000) && --remainingDrains > 0) handler.postDelayed(this, 12)
            } catch (_: Exception) { stop() }
        }
    }
    private fun drain(c: MediaCodec, waitUs: Long = 0): Boolean {
        val info = MediaCodec.BufferInfo()
        var rendered = false
        var pending = c.dequeueOutputBuffer(info, waitUs)
        while (pending != MediaCodec.INFO_TRY_AGAIN_LATER) {
            if (pending >= 0) { c.releaseOutputBuffer(pending, true); rendered = true }
            pending = c.dequeueOutputBuffer(info, 0)
        }
        return rendered
    }
    private fun stop() {
        handler.removeCallbacks(present)
        codec?.let { try { it.stop() } catch (_: Exception) {} finally { it.release() } }
        codec = null; ready = false
    }
    fun reset() { handler.post { stop() } }
    fun close() { handler.post { stop(); main.post { texture.release() }; worker.quitSafely() } }
}
