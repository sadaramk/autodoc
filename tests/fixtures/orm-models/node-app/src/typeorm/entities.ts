import { Column, Entity, JoinColumn, ManyToOne, OneToMany, PrimaryGeneratedColumn } from "typeorm";

export enum InvoiceStatus {
  Draft = "draft",
  Sent = "sent",
  Paid = "paid",
}

@Entity("invoices")
export class Invoice {
  @PrimaryGeneratedColumn("uuid")
  id!: string;

  @Column({ type: "varchar", length: 64, unique: true })
  number!: string;

  @Column({ type: "enum", enum: InvoiceStatus, default: InvoiceStatus.Draft })
  status!: InvoiceStatus;

  @Column({ nullable: true })
  note?: string;

  @OneToMany(() => InvoiceLine, (line) => line.invoice)
  lines!: InvoiceLine[];
}

@Entity("invoice_lines")
export class InvoiceLine {
  @PrimaryGeneratedColumn()
  id!: number;

  @Column("int")
  amountCents!: number;

  @ManyToOne(() => Invoice, (invoice) => invoice.lines)
  @JoinColumn({ name: "invoice_id" })
  invoice!: Invoice;
}
