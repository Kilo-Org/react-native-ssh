package ai.kilo.reactnativessh

import ai.kilo.reactnativessh.terminal.TerminalByteSource
import ai.kilo.reactnativessh.terminal.TerminalSink
import ai.kilo.reactnativessh.uniffi.ByteSink
import ai.kilo.reactnativessh.uniffi.ShellChannel
import ai.kilo.reactnativessh.uniffi.SshException
import ai.kilo.reactnativessh.uniffi.SshSession

/**
 * A shell channel as the terminal view sees it: bytes go straight from Rust to
 * the view, and input comes back the same way. Nothing crosses JavaScript.
 */
internal class SshShellSource private constructor(
  val sessionId: String,
  private val channel: ShellChannel,
  private val delivery: SinkDelivery,
) : TerminalByteSource {
  override fun write(bytes: ByteArray) {
    try {
      channel.write(bytes)
    } catch (_: SshException.ChannelClosed) {
      // The view can still type after the remote shell exits; the close event
      // is the error path.
    }
  }

  override fun resize(cols: Int, rows: Int) {
    try {
      channel.resize(cols.toUInt(), rows.toUInt())
    } catch (_: SshException.ChannelClosed) {
      // See write.
    }
  }

  override fun setSink(sink: TerminalSink?) {
    delivery.bind(sink)
  }

  fun close() {
    try {
      channel.shutdown()
    } catch (_: SshException.ChannelClosed) {
      // Already closed by the server.
    }
  }

  companion object {
    suspend fun open(
      sessionId: String,
      session: SshSession,
      term: String,
      cols: Int,
      rows: Int,
    ): SshShellSource {
      val delivery = SinkDelivery()
      val channel = session.openShell(term, cols.toUInt(), rows.toUInt(), delivery)

      return SshShellSource(sessionId, channel, delivery)
    }
  }
}

/**
 * Bridges the core's output callback to the view's sink.
 *
 * The view attaches after the shell is opened, so bytes that arrive first are
 * buffered up to [MAX_BUFFERED_BYTES] and delivered on `bind`.
 */
internal class SinkDelivery : ByteSink {
  private val lock = Any()
  private var sink: TerminalSink? = null
  private var buffered: MutableList<ByteArray>? = ArrayList()
  private var bufferedBytes = 0
  private var closeEvent: CloseEvent? = null

  fun bind(next: TerminalSink?) {
    val pending: List<ByteArray>
    val closed: CloseEvent?

    synchronized(lock) {
      sink = next
      pending = buffered ?: emptyList()
      closed = closeEvent
      buffered = null
    }

    if (next == null) {
      return
    }

    pending.forEach(next::onData)
    closed?.let { next.onClosed(it.exitStatus, it.error) }
  }

  override fun onData(bytes: ByteArray) {
    val target = synchronized(lock) {
      val current = sink
      if (current == null) {
        buffer(bytes)
      }
      current
    }

    target?.onData(bytes)
  }

  override fun onClosed(exitStatus: UInt?, error: String?) {
    val target = synchronized(lock) {
      val current = sink
      if (current == null) {
        closeEvent = CloseEvent(exitStatus?.toInt(), error)
      }
      current
    }

    target?.onClosed(exitStatus?.toInt(), error)
  }

  private fun buffer(bytes: ByteArray) {
    val pending = buffered ?: return
    if (bufferedBytes + bytes.size > MAX_BUFFERED_BYTES) {
      return
    }

    pending.add(bytes)
    bufferedBytes += bytes.size
  }

  private class CloseEvent(val exitStatus: Int?, val error: String?)

  private companion object {
    const val MAX_BUFFERED_BYTES = 64 * 1024
  }
}
