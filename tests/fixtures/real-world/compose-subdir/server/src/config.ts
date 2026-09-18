/** Base URL of the machine-learning service. */
export function mlUrl(): string {
  return process.env.SHOP_ML_URL || "http://shop-ml:3003";
}
