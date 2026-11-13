package ai.kilo.reactnativessh

import ai.kilo.reactnativessh.background.BackgroundSessions
import ai.kilo.reactnativessh.background.IdleNotifications
import ai.kilo.reactnativessh.terminal.TerminalSourceRegistry
import ai.kilo.reactnativessh.uniffi.AuthMethod
import ai.kilo.reactnativessh.uniffi.ConnectConfig
import ai.kilo.reactnativessh.uniffi.SshException
import ai.kilo.reactnativessh.uniffi.SshSession
import ai.kilo.reactnativessh.uniffi.categoryIsRetryable
import ai.kilo.reactnativessh.uniffi.coreVersion
import ai.kilo.reactnativessh.uniffi.generateEd25519Key
import ai.kilo.reactnativessh.uniffi.parsePrivateKey
import android.content.Context
import expo.modules.kotlin.exception.CodedException
import expo.modules.kotlin.exception.Exceptions
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

internal const val EVENT_HOST_KEY = "onHostKey"
internal const val EVENT_KEYBOARD_INTERACTIVE = "onKeyboardInteractive"
internal const val EVENT_DISCONNECT = "onDisconnect"
internal const val EVENT_SESSIONS_CHANGED = "onSessionsChanged"
internal const val EVENT_COMMAND_FINISHED = "onCommandFinished"
internal const val EVENT_DISCONNECT_ALL = "onDisconnectAll"

class ReactNativeSshModule : Module() {
  private val sessions = ConcurrentHashMap<String, SshSession>()
  private val shells = ConcurrentHashMap<String, SshShellSource>()
  private val hostKeyRequests = ConcurrentHashMap<String, CompletableDeferred<Boolean>>()
  private val promptRequests = ConcurrentHashMap<String, CompletableDeferred<List<String>?>>()
  private val sessionIds = IdSequence("session-")
  private val shellIds = IdSequence("shell-")
  private var reportedSessionCount = 0

  override fun definition() = ModuleDefinition {
    Name("ReactNativeSsh")
    Events(
      EVENT_HOST_KEY,
      EVENT_KEYBOARD_INTERACTIVE,
      EVENT_DISCONNECT,
      EVENT_SESSIONS_CHANGED,
      EVENT_COMMAND_FINISHED,
      EVENT_DISCONNECT_ALL,
    )

    OnCreate {
      BackgroundSessions.disconnectAll = ::disconnectAll
    }

    Function("coreVersion") {
      coreVersion()
    }

    AsyncFunction("generateEd25519Key") { comment: String ->
      val key = translateFailures { generateEd25519Key(comment) }

      GeneratedKeyRecord(key.privateKeyOpenssh, key.publicKeyOpenssh, key.fingerprintSha256)
    }

    Function("parsePrivateKey") { pem: String, passphrase: String? ->
      val key = translateFailures { parsePrivateKey(pem, passphrase) }

      ParsedKeyRecord(
        algorithm = key.algorithm,
        publicKeyOpenssh = key.publicKeyOpenssh,
        fingerprintSha256 = key.fingerprintSha256,
        encrypted = key.encrypted,
      )
    }

    AsyncFunction("connect").SuspendBody { config: ConnectConfigRecord ->
      connectSession(config)
    }

    AsyncFunction("exec").SuspendBody { sessionId: String, command: String ->
      runCommand(sessionId, command)
    }

    AsyncFunction("disconnect").SuspendBody { sessionId: String ->
      disconnectSession(sessionId)
    }

    AsyncFunction("openShell").SuspendBody { sessionId: String, options: ShellOptionsRecord ->
      openShell(sessionId, options)
    }

    Function("closeShell") { shellId: String ->
      closeShell(shellId)
    }

    Function("configureBackgroundSessions") { channelName: String, title: String, disconnectAllLabel: String ->
      BackgroundSessions.configure(androidContext, channelName, title, disconnectAllLabel)
    }

    Function("updateSessionsBody") { body: String ->
      BackgroundSessions.setBody(androidContext, body)
    }

    Function("notifyWhenIdle") { shellId: String, quietMs: Double, title: String, body: String? ->
      val source = shells[shellId]

      if (source == null) {
        false
      } else {
        IdleNotifications.watch(
          context = androidContext,
          shellId = shellId,
          quietMs = quietMs.toLong(),
          activity = source.activity,
          title = title,
          body = body,
          onFinished = { sendEvent(EVENT_COMMAND_FINISHED, mapOf("shellId" to shellId)) },
        )

        true
      }
    }

    Function("answerHostKey") { requestId: String, accept: Boolean ->
      hostKeyRequests[requestId]?.complete(accept) == true
    }

    Function("answerPrompts") { requestId: String, answers: List<String>? ->
      promptRequests[requestId]?.complete(answers) == true
    }

    Function("isRetryableCode") { code: String ->
      categoryIsRetryable(code)
    }

    OnDestroy {
      val scope = CoroutineScope(Dispatchers.IO)

      shells.keys.forEach(::closeShell)
      sessions.forEach { (_, session) -> scope.launch { runCatching { session.disconnect() } } }
      sessions.clear()
      stopBackgroundSessions()
      BackgroundSessions.disconnectAll = null
    }
  }

  private val androidContext: Context
    get() = appContext.reactContext?.applicationContext ?: throw Exceptions.ReactContextLost()

  private suspend fun connectSession(config: ConnectConfigRecord): String {
    val sessionId = sessionIds.next()
    val handler = JsSessionHandler(
      sessionId = sessionId,
      requestHostKey = ::requestHostKey,
      requestPrompts = ::requestPrompts,
      notifyDisconnect = { record ->
        // The module can be gone already when a socket dies during teardown.
        runCatching { sendEvent(EVENT_DISCONNECT, record.toEvent()) }
        removeSession(record.sessionId)
      },
    )

    val session = translateSuspendFailures { SshSession.connect(config.toNative(), handler) }
    sessions[sessionId] = session
    syncSessions()

    return sessionId
  }

  private suspend fun runCommand(sessionId: String, command: String): ExecOutputRecord {
    val session = session(sessionId)
    val result = translateSuspendFailures { session.exec(command) }

    return ExecOutputRecord(
      stdout = result.stdout.toString(Charsets.UTF_8),
      stderr = result.stderr.toString(Charsets.UTF_8),
      exitStatus = result.exitStatus?.toInt(),
    )
  }

  private suspend fun disconnectSession(sessionId: String) {
    val session = sessions.remove(sessionId) ?: return

    closeShells(sessionId)
    syncSessions()
    translateSuspendFailures { session.disconnect() }
  }

  private suspend fun openShell(sessionId: String, options: ShellOptionsRecord): String {
    val session = session(sessionId)
    val shellId = shellIds.next()
    val source = translateSuspendFailures {
      SshShellSource.open(sessionId, session, options.term, options.cols, options.rows)
    }

    shells[shellId] = source
    TerminalSourceRegistry.register(shellId, source)

    return shellId
  }

  private fun closeShell(shellId: String) {
    val source = shells.remove(shellId) ?: return

    IdleNotifications.release(shellId)
    TerminalSourceRegistry.unregister(shellId)
    source.close()
  }

  private fun closeShells(sessionId: String) {
    shells.filterValues { it.sessionId == sessionId }.keys.forEach(::closeShell)
  }

  private fun removeSession(sessionId: String) {
    if (sessions.remove(sessionId) == null) {
      return
    }

    closeShells(sessionId)
    syncSessions()
  }

  private fun syncSessions() {
    val count = sessions.size

    if (count == reportedSessionCount) {
      return
    }

    reportedSessionCount = count
    val context = appContext.reactContext?.applicationContext ?: return

    BackgroundSessions.update(context, count)
    sendEvent(EVENT_SESSIONS_CHANGED, mapOf("count" to count))
  }

  private fun stopBackgroundSessions() {
    val context = appContext.reactContext?.applicationContext ?: return

    BackgroundSessions.update(context, 0)
  }

  private fun disconnectAll() {
    shells.keys.toList().forEach(::closeShell)

    val scope = CoroutineScope(Dispatchers.IO)
    sessions.keys.toList().forEach { sessionId ->
      val session = sessions.remove(sessionId) ?: return@forEach

      scope.launch { runCatching { session.disconnect() } }
    }

    syncSessions()
    sendEvent(EVENT_DISCONNECT_ALL)
  }

  private suspend fun requestHostKey(request: HostKeyRequestRecord): Boolean {
    val decision = CompletableDeferred<Boolean>()
    hostKeyRequests[request.requestId] = decision

    return try {
      sendEvent(EVENT_HOST_KEY, request.toEvent())
      decision.await()
    } finally {
      hostKeyRequests.remove(request.requestId)
    }
  }

  private suspend fun requestPrompts(request: KeyboardInteractiveRequestRecord): List<String>? {
    val answers = CompletableDeferred<List<String>?>()
    promptRequests[request.requestId] = answers

    return try {
      sendEvent(EVENT_KEYBOARD_INTERACTIVE, request.toEvent())
      answers.await()
    } finally {
      promptRequests.remove(request.requestId)
    }
  }

  private fun session(sessionId: String): SshSession =
    sessions[sessionId]
      ?: throw SshException.ChannelClosed("session $sessionId is not connected").toCodedException()
}

/** Runs a core call, translating its failure into a coded JavaScript error. */
private inline fun <T> translateFailures(call: () -> T): T = try {
  call()
} catch (error: SshException) {
  throw error.toCodedException()
}

private suspend fun <T> translateSuspendFailures(call: suspend () -> T): T = try {
  call()
} catch (error: SshException) {
  throw error.toCodedException()
}

private fun AuthRecord.toNative(): AuthMethod = when (type) {
  "password" -> AuthMethod.Password(requireNotNull(password) { "password auth needs a password" })
  "key" -> AuthMethod.Key(
    privateKeyOpenssh = requireNotNull(privateKeyOpenssh) { "key auth needs privateKeyOpenssh" },
    passphrase = passphrase,
  )
  "keyboardInteractive" -> AuthMethod.KeyboardInteractive
  else -> throw CodedException("Protocol", "unknown auth type: $type", null)
}

private fun ConnectConfigRecord.toNative(): ConnectConfig {
  require(port in 1..65535) { "port must be between 1 and 65535" }
  require(connectTimeoutMs > 0) { "connectTimeoutMs must be positive" }
  require(keepaliveIntervalS >= 0) { "keepaliveIntervalS must not be negative" }

  return ConnectConfig(
    host = host,
    port = port.toUShort(),
    username = username,
    auth = auth.toNative(),
    connectTimeoutMs = connectTimeoutMs.toUInt(),
    keepaliveIntervalS = keepaliveIntervalS.toUInt(),
  )
}
