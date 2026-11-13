package ai.kilo.reactnativessh

import ai.kilo.reactnativessh.uniffi.coreVersion as coreVersionNative
import ai.kilo.reactnativessh.uniffi.generateEd25519Key as generateEd25519KeyNative
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

class ReactNativeSshModule : Module() {
  override fun definition() = ModuleDefinition {
    Name("ReactNativeSsh")

    Function("coreVersion") {
      coreVersionNative()
    }

    AsyncFunction("generateEd25519Key") { comment: String ->
      val key = generateEd25519KeyNative(comment)
      mapOf(
        "privateKeyOpenssh" to key.privateKeyOpenssh,
        "publicKeyOpenssh" to key.publicKeyOpenssh,
        "fingerprintSha256" to key.fingerprintSha256,
      )
    }
  }
}
