// A small release helper that ships from the repository root, which is what
// makes the root itself a unit alongside the Spring services below it.
const { execSync } = require("child_process");

/** Tags the current commit. */
function tag(version) {
  execSync(`git tag v${version}`);
}

module.exports = { tag };
