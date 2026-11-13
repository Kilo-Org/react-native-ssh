require 'json'

package = JSON.parse(File.read(File.join(__dir__, '..', 'package.json')))

Pod::Spec.new do |s|
  s.name           = 'ReactNativeSsh'
  s.version        = package['version']
  s.summary        = package['description']
  s.description    = package['description']
  s.license        = package['license']
  s.author         = package['author']
  s.homepage       = package['homepage']
  s.platforms      = {
    :ios => '16.4'
  }
  s.swift_version  = '5.9'
  s.source         = { git: '' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'

  spm_dependency(s,
    url: 'https://github.com/migueldeicaza/SwiftTerm',
    requirement: { kind: 'exactVersion', version: '1.10.1' },
    products: ['SwiftTerm']
  )

  # Swift/Objective-C compatibility
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'HEADER_SEARCH_PATHS' => '"$(PODS_TARGET_SRCROOT)/Generated"',
    'SWIFT_INCLUDE_PATHS' => '"$(PODS_TARGET_SRCROOT)/Generated"',
  }

  s.vendored_frameworks = 'Frameworks/SshCore.xcframework'
  s.source_files = "**/*.{h,m,mm,swift}"
  s.exclude_files = "Frameworks/**/*"
end
