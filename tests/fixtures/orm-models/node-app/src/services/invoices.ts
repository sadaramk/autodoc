import { DataSource } from "typeorm";

import { Invoice, InvoiceStatus } from "../typeorm/entities";

/** Sends a draft invoice. */
export async function sendInvoice(ds: DataSource, id: string) {
  const repo = ds.getRepository(Invoice);
  const invoice = await repo.findOneByOrFail({ id });
  if (invoice.status !== InvoiceStatus.Draft) {
    throw new Error("only drafts can be sent");
  }
  invoice.status = InvoiceStatus.Sent;
  await repo.save(invoice);
}

/** Records payment of a sent invoice. */
export async function markInvoicePaid(ds: DataSource, id: string) {
  const repo = ds.getRepository(Invoice);
  await repo.update({ id, status: InvoiceStatus.Sent }, { status: InvoiceStatus.Paid });
}
