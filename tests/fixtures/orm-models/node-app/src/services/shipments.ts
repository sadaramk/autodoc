export type ShipmentState = "created" | "in_transit" | "delivered";

export interface Shipment {
  id: string;
  state: ShipmentState;
}

/** Marks a shipment delivered when the carrier confirms it. */
export function confirmDelivery(shipment: Shipment): Shipment {
  if (shipment.state === "in_transit") {
    shipment.state = "delivered";
  }
  return shipment;
}
