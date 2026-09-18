import cron from "node-cron";
import { expireCarts } from "./db";

/** Hourly housekeeping. */
export function startJobs(): void {
  cron.schedule("0 * * * *", expireCarts);
}
