const path = require("node:path");

module.exports = {
  apps: [
    {
      name: "vps-back",
      // Compiled release binary (cargo build --release).
      script: path.join(__dirname, "target/release/vps-back"),
      cwd: __dirname,
      // Native binary, not a Node script.
      interpreter: "none",

      // The app reads .env itself (dotenvy), so nothing is duplicated here.
      // Only pm2 level overrides go in env / env_production.
      env: {
        RUST_LOG: "info",
      },
      env_production: {
        RUST_LOG: "info",
      },

      instances: 1,
      exec_mode: "fork",
      autorestart: true,
      watch: false,
      max_memory_restart: "300M",
      min_uptime: "10s",
      max_restarts: 10,
      restart_delay: 2000,
      kill_timeout: 5000,

      merge_logs: true,
      time: true,
      out_file: path.join(__dirname, "logs/vps-back.out.log"),
      error_file: path.join(__dirname, "logs/vps-back.err.log"),
    },
  ],
};
