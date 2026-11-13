import ExpoModulesCore

/**
 * Payloads the module hands to JavaScript.
 *
 * Incoming values are Expo records; outgoing ones are plain structs with the
 * same shape as the Kotlin records, because `sendEvent` takes a dictionary.
 */
struct ConnectConfigRecord: Record, @unchecked Sendable {
  @Field var host: String = ""
  @Field var port: Int = 22
  @Field var username: String = ""
  @Field var auth: AuthRecord = AuthRecord()
  @Field var connectTimeoutMs: Int = 15_000
  @Field var keepaliveIntervalS: Int = 15
}

struct AuthRecord: Record, @unchecked Sendable {
  @Field var type: String = ""
  @Field var password: String?
  @Field var privateKeyOpenssh: String?
  @Field var passphrase: String?
}

struct ShellOptionsRecord: Record, @unchecked Sendable {
  @Field var term: String = "xterm-256color"
  @Field var cols: Int = 80
  @Field var rows: Int = 24
}

struct GeneratedKeyRecord: Record, @unchecked Sendable {
  @Field var privateKeyOpenssh: String = ""
  @Field var publicKeyOpenssh: String = ""
  @Field var fingerprintSha256: String = ""
}

struct ParsedKeyRecord: Record, @unchecked Sendable {
  @Field var algorithm: String = ""
  @Field var publicKeyOpenssh: String = ""
  @Field var fingerprintSha256: String = ""
  @Field var encrypted: Bool = false
}

struct ExecOutputRecord: Record, @unchecked Sendable {
  @Field var stdout: String = ""
  @Field var stderr: String = ""
  @Field var exitStatus: Int?
}

struct HostKeyRequest: Sendable {
  let sessionId: String
  let requestId: String
  let host: String
  let port: Int
  let algorithm: String
  let publicKeyBase64: String
  let fingerprintSha256: String

  func toEvent() -> [String: Any?] {
    [
      "sessionId": sessionId,
      "requestId": requestId,
      "host": host,
      "port": port,
      "algorithm": algorithm,
      "publicKeyBase64": publicKeyBase64,
      "fingerprintSha256": fingerprintSha256
    ]
  }
}

struct PromptPayload: Sendable {
  let text: String
  let echo: Bool

  func toEvent() -> [String: Any?] {
    ["text": text, "echo": echo]
  }
}

struct KeyboardInteractiveRequest: Sendable {
  let sessionId: String
  let requestId: String
  let name: String
  let instructions: String
  let prompts: [PromptPayload]

  func toEvent() -> [String: Any?] {
    [
      "sessionId": sessionId,
      "requestId": requestId,
      "name": name,
      "instructions": instructions,
      "prompts": prompts.map { $0.toEvent() }
    ]
  }
}

struct DisconnectNotice: Sendable {
  let sessionId: String
  let reason: String

  func toEvent() -> [String: Any?] {
    ["sessionId": sessionId, "reason": reason]
  }
}
