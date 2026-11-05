import Foundation
public enum WordStyle: Sendable, Equatable {
 case plain
 case sound // Onomatopoeia and interjections: "haha", "whoa", "kaboom".
 case slang // Internet slang and memes: "bruh", "no cap".
 case cue   // A non-speech sound Whisper reported: "[laughter]".
}
// Maps Whisper's non-speech notes to a short label. Anything else, such as [BLANK_AUDIO] or
// (speaking foreign language), is dropped, because those notes are mostly noise.
public func cueLabel(for note: String) -> String? {
 let text = note.lowercased().filter { $0.isLetter || $0 == " " || $0 == "♪" || $0 == "♫" }
 if text.contains("♪") || text.contains("♫") || text.contains("music") || text.contains("singing") { return "music" }
 if text.contains("laugh") || text.contains("chuckl") || text.contains("giggl") { return "laughter" }
 if text.contains("applau") || text.contains("clapping") { return "applause" }
 if text.contains("cheer") { return "cheering" }
 return nil
}
// Decides which words stand out, on the principle that when everything stands out nothing
// does. The lists are short, and everyday fillers ("um", "oh", "yeah") are left plain on purpose.
private let sounds: Set<String> = [
 "aha", "ahem", "argh", "aww", "bam", "beep", "blah", "boing", "boop", "brr", "chaching", "clang", "clank", "cowabunga",
 "eek", "eureka", "eww", "gah", "grr", "haha", "hehe", "hooray", "kaboom", "kablam", "kachow", "kapow", "kerplunk", "meow",
 "mwahaha", "oink", "oof", "ooh", "oops", "ouch", "ow", "phew", "pow", "psst", "ribbit", "shh", "splat", "swoosh", "tada", "thud",
 "uhoh", "ugh", "vroom", "wham", "whee", "whoa", "whoops", "whoosh", "woah", "woof", "woohoo", "wow", "yay", "yikes", "yippee", "zing",
]
private let slang: Set<String> = [
 "bruh", "bussin", "delulu", "goated", "gyatt", "iykyk", "lmao", "lol", "omg", "pog", "poggers", "rizz", "rizzler", "rofl",
 "sheesh", "skibidi", "smh", "sus", "wtf", "yeet",
]
// Everyday words that only count when shouted: "Boom!" but not "the baby boom".
private let shoutedSounds: Set<String> = ["bang", "boom", "buzz", "clap", "crash", "crack", "ding", "hiss", "pop", "roar", "slam", "smack", "snap", "splash", "thump", "whack", "zap"]
private let shoutedSlang: Set<String> = ["based", "bet", "bro", "cap", "cringe", "facts", "fire", "lit", "mid", "periodt", "slay", "sigma"]
private let slangPhrases: [[String]] = [["big", "mood"], ["its", "giving"], ["lets", "go"], ["main", "character"], ["mic", "drop"], ["no", "cap"], ["plot", "twist"], ["say", "less"], ["skill", "issue"], ["touch", "grass"]]
private let shoutedPhrases: [[String]] = [["no", "way"], ["oh", "my", "god"], ["oh", "no"]]
private let soundPhrases: [[String]] = [["blah", "blah"], ["ding", "dong"], ["la", "la"], ["na", "na"], ["tick", "tock"], ["yada", "yada"]]
private func isLaugh(_ key: String) -> Bool {
 key.range(of: "^(a*h+a+|h+e+|h+i+){2,}h*$", options: .regularExpression) != nil
}
// "Sooooo" and "nooo" are spoken for effect. Numbers and short words are not.
private func isStretched(_ key: String) -> Bool {
 key.count >= 4 && key.allSatisfy(\.isLetter) && key.range(of: "(.)\\1\\1", options: .regularExpression) != nil
}
public func wordStyles(_ words: [String]) -> [WordStyle] {
 let keys = words.map { $0.lowercased().filter { $0.isLetter || $0.isNumber } }
 var styles = [WordStyle](repeating: .plain, count: words.count)
 // A word is shouted if it has an exclamation mark or stands alone as a short sentence.
 func shouted(_ i: Int) -> Bool {
  if words[i].hasSuffix("!") { return true }
  let opens = i == 0 || ".?!".contains(words[i-1].last ?? " ")
  return opens && ".?!".contains(words[i].last ?? " ")
 }
 for i in words.indices {
  if words[i].hasPrefix("[") { styles[i] = .cue; continue }
  let key = keys[i]
  if key.isEmpty { continue }
  if sounds.contains(key) || isLaugh(key) || isStretched(key) { styles[i] = .sound }
  else if slang.contains(key) { styles[i] = .slang }
  else if shouted(i) { if shoutedSounds.contains(key) { styles[i] = .sound } else if shoutedSlang.contains(key) { styles[i] = .slang } }
 }
 func mark(_ phrases: [[String]], _ style: WordStyle, shoutedOnly: Bool = false) {
  for phrase in phrases where phrase.count <= keys.count {
   for start in 0...(keys.count - phrase.count) where Array(keys[start..<start+phrase.count]) == phrase {
    if shoutedOnly, !shouted(start + phrase.count - 1) { continue }
    for i in start..<start+phrase.count { styles[i] = style }
   }
  }
 }
 mark(soundPhrases, .sound); mark(slangPhrases, .slang); mark(shoutedPhrases, .sound, shoutedOnly: true)
 // "ha ha ha": a laugh spoken as separate words.
 for i in words.indices where ["ha", "he", "ho", "hah"].contains(keys[i]) {
  if (i > 0 && keys[i-1] == keys[i]) || (i + 1 < keys.count && keys[i+1] == keys[i]) { styles[i] = .sound }
 }
 return styles
}
