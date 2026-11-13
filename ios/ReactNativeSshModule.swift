import ExpoModulesCore

public class ReactNativeSshModule: Module {
  public func definition() -> ModuleDefinition {
    Name("ReactNativeSsh")

    Function("coreVersion") { () -> String in
      coreVersion()
    }

    AsyncFunction("generateEd25519Key") { (comment: String) throws -> [String: String] in
      let key = try generateEd25519Key(comment: comment)
      return [
        "privateKeyOpenssh": key.privateKeyOpenssh,
        "publicKeyOpenssh": key.publicKeyOpenssh,
        "fingerprintSha256": key.fingerprintSha256,
      ]
    }
  }
}
