/* Generated from Rust policy limits. Do not edit. */
export const executionLimitRules = {
  "duration": {
    "min": 1,
    "max": 4294967295
  },
  "concurrency": {
    "min": 1,
    "max": 16
  },
  "defaults": {
    "concurrentTasks": 4,
    "concurrentTerminals": 2,
    "concurrentTransfers": 2,
    "maxTaskSeconds": 1800,
    "terminalIdleSeconds": null,
    "terminalSeconds": 7200,
    "transferSeconds": null,
    "transferStallSeconds": 300
  }
} as const;
