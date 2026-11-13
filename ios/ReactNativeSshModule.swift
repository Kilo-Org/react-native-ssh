import ExpoModulesCore

public final class ReactNativeSshModule: Module {
  private let state = SshModuleState()

  public func definition() -> ModuleDefinition {
    Name("ReactNativeSsh")
    Events("onHostKey", "onKeyboardInteractive", "onDisconnect")

    Function("coreVersion") { () -> String in
      coreVersion()
    }

    AsyncFunction("generateEd25519Key") { (comment: String) throws -> GeneratedKeyRecord in
      let key = try translate { try generateEd25519Key(comment: comment) }

      return GeneratedKeyRecord(
        privateKeyOpenssh: key.privateKeyOpenssh,
        publicKeyOpenssh: key.publicKeyOpenssh,
        fingerprintSha256: key.fingerprintSha256
      )
    }

    Function("parsePrivateKey") { (pem: String, passphrase: String?) throws -> ParsedKeyRecord in
      let key = try translate { try parsePrivateKey(pem: pem, passphrase: passphrase) }

      return ParsedKeyRecord(
        algorithm: key.algorithm,
        publicKeyOpenssh: key.publicKeyOpenssh,
        fingerprintSha256: key.fingerprintSha256,
        encrypted: key.encrypted
      )
    }

    AsyncFunction("connect") { (config: ConnectConfigRecord) async throws -> String in
      try await self.connectSession(config)
    }

    AsyncFunction("exec") { (sessionId: String, command: String) async throws -> ExecOutputRecord in
      try await self.runCommand(sessionId, command)
    }

    AsyncFunction("disconnect") { (sessionId: String) async throws in
      try await self.disconnectSession(sessionId)
    }

    AsyncFunction("openShell") { (sessionId: String, options: ShellOptionsRecord) async throws -> String in
      try await self.openShell(sessionId, options)
    }

    Function("closeShell") { (shellId: String) in
      self.closeShell(shellId)
    }

    Function("answerHostKey") { (requestId: String, accept: Bool) -> Bool in
      self.state.takeHostKeyRequest(requestId)?.resolve(accept) ?? false
    }

    Function("answerPrompts") { (requestId: String, answers: [String]?) -> Bool in
      let reply = answers.map(PromptReply.answers) ?? .cancelled

      return self.state.takePromptRequest(requestId)?.resolve(reply) ?? false
    }

    Function("isRetryableCode") { (code: String) -> Bool in
      self.isRetryable(code)
    }

    OnDestroy {
      self.state.allShellIds().forEach { self.closeShell($0) }
      Task {
        for session in self.state.allSessions() {
          try? await session.disconnect()
        }
      }
    }
  }

  private func isRetryable(_ code: String) -> Bool {
    categoryIsRetryable(category: code)
  }

  private func connectSession(_ config: ConnectConfigRecord) async throws -> String {
    let sessionId = state.sessionIds.next()
    let handler = JsSessionHandler(
      sessionId: sessionId,
      requestHostKey: { [weak self] request in
        await self?.requestHostKey(request) ?? false
      },
      requestPrompts: { [weak self] request in
        await self?.requestPrompts(request) ?? .cancelled
      },
      notifyDisconnect: { [weak self] notice in
        self?.sendEvent("onDisconnect", notice.toEvent())
      }
    )

    let session = try await translateAsync { try await SshSession.connect(config: config.toNative(), handler: handler) }
    state.store(session: session, id: sessionId)

    return sessionId
  }

  private func runCommand(_ sessionId: String, _ command: String) async throws -> ExecOutputRecord {
    guard let session = state.session(sessionId) else {
      throw channelClosed(sessionId)
    }

    let result = try await translateAsync { try await session.exec(command: command) }

    return ExecOutputRecord(
      stdout: String(decoding: result.stdout, as: UTF8.self),
      stderr: String(decoding: result.stderr, as: UTF8.self),
      exitStatus: result.exitStatus.map { Int($0) }
    )
  }

  private func disconnectSession(_ sessionId: String) async throws {
    guard let session = state.removeSession(sessionId) else {
      return
    }

    state.shellIds(of: sessionId).forEach { closeShell($0) }
    try await translateAsync { try await session.disconnect() }
  }

  private func openShell(_ sessionId: String, _ options: ShellOptionsRecord) async throws -> String {
    guard let session = state.session(sessionId) else {
      throw channelClosed(sessionId)
    }

    let shellId = state.shellIds.next()
    let source = try await translateAsync {
      try await SshShellSource.open(
        sessionId: sessionId,
        session: session,
        term: options.term,
        cols: options.cols,
        rows: options.rows
      )
    }

    state.store(shell: source, id: shellId)
    TerminalSourceRegistry.shared.register(shellId, source)

    return shellId
  }

  private func closeShell(_ shellId: String) {
    guard let source = state.removeShell(shellId) else {
      return
    }

    TerminalSourceRegistry.shared.unregister(shellId)
    source.close()
  }

  private func requestHostKey(_ request: HostKeyRequest) async -> Bool {
    let pending = PendingRequest<Bool>()
    state.store(hostKeyRequest: pending, id: request.requestId)

    defer { _ = state.takeHostKeyRequest(request.requestId) }
    sendEvent("onHostKey", request.toEvent())

    return await pending.wait()
  }

  private func requestPrompts(_ request: KeyboardInteractiveRequest) async -> PromptReply {
    let pending = PendingRequest<PromptReply>()
    state.store(promptRequest: pending, id: request.requestId)

    defer { _ = state.takePromptRequest(request.requestId) }
    sendEvent("onKeyboardInteractive", request.toEvent())

    return await pending.wait()
  }
}

private func channelClosed(_ sessionId: String) -> Exception {
  Exception(name: "SshError", description: "session \(sessionId) is not connected", code: "ChannelClosed")
}

private func translate<Value>(_ call: () throws -> Value) throws -> Value {
  do {
    return try call()
  } catch let error as SshError {
    throw error.codedException
  }
}

private func translateAsync<Value>(_ call: () async throws -> Value) async throws -> Value {
  do {
    return try await call()
  } catch let error as SshError {
    throw error.codedException
  }
}

private extension ConnectConfigRecord {
  func toNative() throws -> ConnectConfig {
    guard port >= 1 && port <= 65535 else {
      throw Exception(name: "SshError", description: "port must be between 1 and 65535", code: "Protocol")
    }
    guard connectTimeoutMs > 0, keepaliveIntervalS >= 0 else {
      throw Exception(name: "SshError", description: "invalid timeout or keepalive", code: "Protocol")
    }

    return ConnectConfig(
      host: host,
      port: UInt16(port),
      username: username,
      auth: try auth.toNative(),
      connectTimeoutMs: UInt32(connectTimeoutMs),
      keepaliveIntervalS: UInt32(keepaliveIntervalS)
    )
  }
}

private extension AuthRecord {
  func toNative() throws -> AuthMethod {
    switch type {
    case "password":
      guard let password else {
        throw Exception(name: "SshError", description: "password auth needs a password", code: "Protocol")
      }
      return .password(password: password)
    case "key":
      guard let privateKeyOpenssh else {
        throw Exception(name: "SshError", description: "key auth needs privateKeyOpenssh", code: "Protocol")
      }
      return .key(privateKeyOpenssh: privateKeyOpenssh, passphrase: passphrase)
    case "keyboardInteractive":
      return .keyboardInteractive
    default:
      throw Exception(name: "SshError", description: "unknown auth type: \(type)", code: "Protocol")
    }
  }
}
