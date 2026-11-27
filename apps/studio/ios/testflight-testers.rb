#!/usr/bin/env ruby
# Hands an uploaded build to Studio's TestFlight testers, through the App
# Store Connect API. The release workflow runs it after the upload
# (desktop.yml, job `testflight`):
#
#   1. waits for Apple to finish processing the build (VALID);
#   2. writes its "What to Test": the release and where its notes are;
#   3. internal groups: the build is theirs. A group with access to all
#      builds gets it by itself; any other one has it added;
#   4. external groups (TESTFLIGHT_GROUPS, default "Testers", the one with
#      the public link): the build is added, then submitted for Beta App
#      Review. Its testers can install it once Apple approves (often
#      within minutes for a later build of an approved app; up to a day
#      otherwise). TestFlight notifies every tester of a new build.
#
#   ruby testflight-testers.rb <bundle-id> <build-number> <version> [notes-url]
#
# DRY_RUN=1 reads and reports and changes nothing (no wait either: a
# build that is not processed yet is reported as such). WAIT_MIN bounds the
# wait for processing (default 60 minutes).
#
# What only App Store Connect's owner can set stops at step 4 with a
# warning and an exit of 0: the internal testers have the build already,
# and the release is not held for the external review (for example, the
# app's Beta App Review contact details, empty until the owner fills them
# in: TestFlight > Test Information). Anything else that fails exits 1.
#
# Reads ASC_KEY_ID, ASC_ISSUER_ID and ASC_KEY_PATH from the environment
# (source ~/.appstoreconnect/config.env). Prints no credential.

require "openssl"
require "json"
require "base64"
require "net/http"
require "uri"

BUNDLE_ID, BUILD_NO, VERSION, NOTES_URL = ARGV
abort("usage: testflight-testers.rb <bundle-id> <build-number> <version> [notes-url]") \
  unless BUNDLE_ID && BUILD_NO =~ /\A\d+\z/ && VERSION =~ /\A\d+\.\d+\.\d+\z/
DRY = ENV["DRY_RUN"] == "1"
WAIT_MIN = Integer(ENV.fetch("WAIT_MIN", "60"))
EXTERNAL = ENV.fetch("TESTFLIGHT_GROUPS", "Testers").split(",").map(&:strip).reject(&:empty?)

KEY_ID = ENV.fetch("ASC_KEY_ID")
ISSUER_ID = ENV.fetch("ASC_ISSUER_ID")
KEY_PATH = ENV.fetch("ASC_KEY_PATH")

# A GitHub annotation when run in Actions, plain text otherwise.
def warn_gh(msg)
  puts(ENV["GITHUB_ACTIONS"] ? "::warning::#{msg}" : "WARNING: #{msg}")
end

# An App Store Connect token lives 20 minutes at most; the wait for
# processing can outlast one, so each request takes a fresh one when the
# last is near its end.
def token
  now = Time.now.to_i
  return @token if @token && now < @token_exp - 60
  seg = ->(h) { Base64.urlsafe_encode64(JSON.dump(h), padding: false) }
  input = "#{seg.call({ alg: "ES256", kid: KEY_ID, typ: "JWT" })}." \
          "#{seg.call({ iss: ISSUER_ID, iat: now, exp: now + 900, aud: "appstoreconnect-v1" })}"
  der = OpenSSL::PKey::EC.new(File.read(KEY_PATH)).sign(OpenSSL::Digest.new("SHA256"), input)
  a = OpenSSL::ASN1.decode(der)
  r = a.value[0].value.to_s(2).rjust(32, "\x00")
  s = a.value[1].value.to_s(2).rjust(32, "\x00")
  @token_exp = now + 900
  @token = "#{input}.#{Base64.urlsafe_encode64(r + s, padding: false)}"
end

# The request's status and parsed body. soft: a failure is returned
# ({"_error" => code, "_body" => ...}) instead of ending the run.
def api(method, path, body = nil, soft: false)
  uri = URI("https://api.appstoreconnect.apple.com#{path}")
  cls = { get: Net::HTTP::Get, post: Net::HTTP::Post, patch: Net::HTTP::Patch }.fetch(method)
  req = cls.new(uri)
  req["Authorization"] = "Bearer #{token}"
  req["Content-Type"] = "application/json"
  req.body = JSON.dump(body) if body
  res = Net::HTTP.start(uri.host, uri.port, use_ssl: true, read_timeout: 60) { |h| h.request(req) }
  unless res.code.to_i.between?(200, 299)
    return { "_error" => res.code, "_body" => res.body.to_s[0, 2000] } if soft
    abort("App Store Connect: #{method.upcase} #{uri.path} -> #{res.code}\n#{res.body.to_s[0, 2000]}")
  end
  res.body.to_s.empty? ? {} : JSON.parse(res.body)
end

# Apple's first error, readably.
def reason(res)
  JSON.parse(res["_body"]).dig("errors", 0).then { |e| e ? "#{e["title"]}: #{e["detail"]}" : res["_body"] }
rescue JSON::ParserError
  res["_body"]
end

app = api(:get, "/v1/apps?filter[bundleId]=#{BUNDLE_ID}&limit=5")["data"]
  .find { |a| a["attributes"]["bundleId"] == BUNDLE_ID } or abort("no App Store Connect app for #{BUNDLE_ID}")
app_id = app["id"]
puts "app: #{app["attributes"]["name"]} (#{BUNDLE_ID})#{DRY ? " -- dry run, nothing is changed" : ""}"

# 1. The build, once processed. An upload takes a few minutes to appear
# at all, then 5-15 to process.
deadline = Time.now + WAIT_MIN * 60
build = nil
loop do
  build = api(:get, "/v1/builds?filter[app]=#{app_id}&filter[version]=#{BUILD_NO}&limit=1")["data"].first
  state = build&.dig("attributes", "processingState")
  case state
  when "VALID" then break
  when "INVALID", "FAILED" then abort("build #{BUILD_NO}: Apple's processing ended #{state}: see App Store Connect's email")
  end
  if DRY
    puts "build #{BUILD_NO}: #{state || "not in App Store Connect yet"}: a real run would wait for it (up to #{WAIT_MIN} min)"
    exit 0
  end
  abort("build #{BUILD_NO}: still #{state || "not in App Store Connect"} after #{WAIT_MIN} minutes") if Time.now > deadline
  puts "build #{BUILD_NO}: #{state || "not in App Store Connect yet"}; waiting"
  sleep 30
end
build_id = build["id"]
puts "build #{BUILD_NO}: VALID"

# 2. What to Test: the testers see it in the TestFlight app with the build.
whats_new = "Studio #{VERSION} (build #{BUILD_NO})." + (NOTES_URL ? " What changed: #{NOTES_URL}" : "")
locs = api(:get, "/v1/builds/#{build_id}/betaBuildLocalizations")["data"]
en = locs.find { |l| l["attributes"]["locale"] == "en-US" }
if DRY
  puts "would set What to Test (en-US): #{whats_new}"
elsif en
  api(:patch, "/v1/betaBuildLocalizations/#{en["id"]}",
      { data: { type: "betaBuildLocalizations", id: en["id"], attributes: { whatsNew: whats_new } } })
  puts "What to Test set"
else
  api(:post, "/v1/betaBuildLocalizations",
      { data: { type: "betaBuildLocalizations", attributes: { locale: "en-US", whatsNew: whats_new },
                relationships: { build: { data: { type: "builds", id: build_id } } } } })
  puts "What to Test set"
end

groups = api(:get, "/v1/apps/#{app_id}/betaGroups?limit=200")["data"]
add_to = lambda do |g|
  name = g["attributes"]["name"]
  if DRY
    puts "would add build #{BUILD_NO} to '#{name}'"
    return true
  end
  res = api(:post, "/v1/betaGroups/#{g["id"]}/relationships/builds",
            { data: [{ type: "builds", id: build_id }] }, soft: true)
  if res["_error"]
    warn_gh("could not add build #{BUILD_NO} to TestFlight group '#{name}': #{reason(res)}")
    return false
  end
  puts "added to '#{name}'"
  true
end

# 3. Internal testers (members of the App Store Connect team): no review.
internal = groups.select { |g| g["attributes"]["isInternalGroup"] }
abort("no internal TestFlight group for #{BUNDLE_ID}: create one in App Store Connect") if internal.empty?
internal.each do |g|
  if g["attributes"]["hasAccessToAllBuilds"]
    puts "internal '#{g["attributes"]["name"]}': has every build"
  else
    add_to.call(g) or abort("internal group '#{g["attributes"]["name"]}' did not get build #{BUILD_NO}")
  end
end

# 4. External testers: the group(s), then Beta App Review.
external = groups.reject { |g| g["attributes"]["isInternalGroup"] }
               .select { |g| EXTERNAL.include?(g["attributes"]["name"]) }
missing = EXTERNAL - external.map { |g| g["attributes"]["name"] }
warn_gh("no external TestFlight group named #{missing.join(", ")} for #{BUNDLE_ID}") unless missing.empty?
added = external.map { |g| add_to.call(g) }.any?
exit 0 if external.empty? || !added

sub = api(:get, "/v1/builds/#{build_id}/betaAppReviewSubmission", nil, soft: true)
state = sub.dig("data", "attributes", "betaReviewState")
if state
  puts "Beta App Review: #{state}"
  exit 0
end
detail = api(:get, "/v1/apps/#{app_id}/betaAppReviewDetail", nil, soft: true).dig("data", "attributes") || {}
unset = %w[contactFirstName contactLastName contactEmail contactPhone].select { |k| detail[k].to_s.empty? }
unless unset.empty?
  warn_gh("build #{BUNDLE_ID} #{BUILD_NO} is not submitted for Beta App Review, so external testers do not get it yet: " \
          "the app's review contact (#{unset.join(", ")}) is empty. The owner fills it in once " \
          "(App Store Connect > TestFlight > Test Information), then re-runs this job.")
  exit 0
end
if DRY
  puts "would submit build #{BUILD_NO} for Beta App Review"
  exit 0
end
res = api(:post, "/v1/betaAppReviewSubmissions",
          { data: { type: "betaAppReviewSubmissions",
                    relationships: { build: { data: { type: "builds", id: build_id } } } } }, soft: true)
if res["_error"]
  warn_gh("build #{BUILD_NO}: the Beta App Review submission was refused (#{res["_error"]}): #{reason(res)}")
else
  puts "Beta App Review: submitted (#{res.dig("data", "attributes", "betaReviewState")}); " \
       "external testers get build #{BUILD_NO} once Apple approves it"
end
