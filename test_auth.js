const fs = require("fs");
const os = require("os");
const path = require("path");
const p = path.join(os.homedir(), ".local/share/opencode/auth.json");
const data = JSON.parse(fs.readFileSync(p, "utf8"));
data["anthropic"] = { type: "api", key: "sk-test-anthropic" };
fs.writeFileSync(p, JSON.stringify(data, null, 2));
