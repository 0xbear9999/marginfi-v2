import argparse
import base64
import json
import logging
from dataclasses import asdict
from typing import List, Optional, Sequence, Union, Generator, Any, Tuple, Dict
from anchorpy import NamedInstruction
from solders.message import MessageV0, Message
import apache_beam as beam  # type: ignore
from apache_beam.options.pipeline_options import PipelineOptions  # type: ignore
from solders.pubkey import Pubkey

from dataflow_etls.orm.events import Record, EVENT_TO_RECORD_TYPE, RecordTypes
from dataflow_etls.idl_versions import VersionedIdl, VersionedProgram, Cluster
from dataflow_etls.transaction_log_parser import reconcile_instruction_logs, \
    merge_instructions_and_cpis, expand_instructions, InstructionWithLogs, PROGRAM_DATA


class DispatchEventsDoFn(beam.DoFn):  # type: ignore
    def process(self, record: Record, *args: Tuple[Any], **kwargs: Dict[str, Tuple[Any]]) -> Generator[str, None, None]:
        yield beam.pvalue.TaggedOutput(record.get_tag(), record)


def create_records_from_ix(ix: InstructionWithLogs, program: VersionedProgram) -> Sequence[Record]:
    records: List[Record] = []

    try:
        parsed_ix: NamedInstruction = program.coder.instruction.parse(ix.message.data)
    except Exception as e:
        print(ix)
        print(f"failed to parse instruction data in tx {ix.signature}", e)
        return records

    for log in ix.logs:
        if not log.startswith(PROGRAM_DATA):
            continue

        event_encoded = log[len(PROGRAM_DATA):]
        try:
            event_bytes = base64.b64decode(event_encoded)
        except Exception as e:
            print(f"error: failed to decode base64 event string in tx {ix.signature}", e)
            continue

        print(f"info decoded with IDL {program.version}")

        try:
            event = program.coder.events.parse(event_bytes)
        except Exception as e:
            print(f"failed to parse event in tx {ix.signature}", e)
            continue

        if event is None or event.name not in EVENT_TO_RECORD_TYPE:
            print(f"discarding unsupported event in tx {ix.signature}")
            print(event)
        else:
            # noinspection PyPep8Naming
            RecordType = EVENT_TO_RECORD_TYPE[event.name]
            records.append(RecordType(event, ix, parsed_ix))

    return records


def extract_events_from_ix(ix: InstructionWithLogs, program: VersionedProgram) -> List[Record]:
    ix_events: List[Record] = []

    if ix.message.program_id == program.program_id:
        ix_events.extend(create_records_from_ix(ix, program))

    for inner_ix in ix.inner_instructions:
        ix_events.extend(extract_events_from_ix(inner_ix, program))

    return ix_events


def dictionify_record(record: Record) -> Dict[str, Any]:
    return asdict(record)


def run(
        input_table: str,
        output_table_namespace: str,
        cluster: Cluster,
        min_idl_version: int,
        start_date: Optional[str] = None,
        end_date: Optional[str] = None,
        beam_args: Optional[List[str]] = None,
) -> None:
    if beam_args is None:
        beam_args = []

    def extract_events_from_tx(tx: Any) -> List[Record]:
        indexed_program_id_str = tx["indexing_address"]
        indexed_program_id = Pubkey.from_string(indexed_program_id_str)
        tx_slot = int(tx["slot"])
        idl, idl_version = VersionedIdl.get_idl_for_slot(cluster, indexed_program_id_str, tx_slot)
        program = VersionedProgram(cluster, idl_version, idl, indexed_program_id)

        if min_idl_version is not None and idl_version < min_idl_version:
            return []

        meta = json.loads(tx["meta"])
        message_bytes = base64.b64decode(tx["message"])

        tx_version = tx["version"]
        message_decoded: Union[Message, MessageV0]
        if tx_version == "legacy":
            message_decoded = Message.from_bytes(message_bytes)
        elif tx_version == "0":
            message_decoded = MessageV0.from_bytes(message_bytes[1:])
        else:
            return []

        merged_instructions = merge_instructions_and_cpis(message_decoded.instructions, meta["innerInstructions"])
        expanded_instructions = expand_instructions(message_decoded.account_keys, merged_instructions)
        ixs_with_logs = reconcile_instruction_logs(tx["timestamp"], tx["signature"], expanded_instructions,
                                                   meta["logMessages"], idl_version)

        records_list = []
        for ix_with_logs in ixs_with_logs:
            records_list.extend(extract_events_from_ix(ix_with_logs, program))

        return records_list

    """Build and run the pipeline."""
    pipeline_options = PipelineOptions(beam_args, save_main_session=True)

    if start_date is not None and end_date is not None:
        input_query = f'SELECT * FROM `{input_table}` WHERE DATE(timestamp) >= "{start_date}" AND DATE(timestamp) < "{end_date}"'
    elif start_date is not None:
        input_query = (
            f'SELECT * FROM `{input_table}` WHERE DATE(timestamp) >= "{start_date}"'
        )
    elif end_date is not None:
        input_query = (
            f'SELECT * FROM `{input_table}` WHERE DATE(timestamp) < "{end_date}"'
        )
    else:
        input_query = f"SELECT * FROM `{input_table}`"

    with beam.Pipeline(options=pipeline_options) as pipeline:
        # Define steps
        read_raw_txs = beam.io.ReadFromBigQuery(query=input_query, use_standard_sql=True)

        extract_events = beam.FlatMap(extract_events_from_tx)

        dispatch_events = beam.ParDo(DispatchEventsDoFn()).with_outputs(*[rt.get_tag() for rt in RecordTypes])

        dictionify_events = beam.Map(dictionify_record)

        writers: Dict[str, Union[beam.io.WriteToText, beam.io.WriteToBigQuery]] = {}
        for rt in RecordTypes:
            if output_table_namespace == "local_file":  # For testing purposes
                writers[rt.get_tag()] = beam.io.WriteToText(f"parsed_event_{rt.get_tag()}")
            else:
                writers[rt.get_tag()] = beam.io.WriteToBigQuery(
                    f"{output_table_namespace}_{rt.get_tag(snake_case=True)}",
                    schema=rt.SCHEMA,
                    write_disposition=beam.io.BigQueryDisposition.WRITE_APPEND,
                    create_disposition=beam.io.BigQueryDisposition.CREATE_IF_NEEDED,
                )

        # Define pipeline
        tagged_events = (
                pipeline
                | "ReadRawTxs" >> read_raw_txs
                | "ExtractEvents" >> extract_events
                | "DispatchEvents" >> dispatch_events
        )

        for rt in RecordTypes:
            (tagged_events[rt.get_tag()]
                | f"Dictionify{rt.get_tag()}" >> dictionify_events
                | f"Write{rt.get_tag()}" >> writers[rt.get_tag()]
            )


def main() -> None:
    logging.getLogger().setLevel(logging.INFO)

    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--input_table",
        type=str,
        required=True,
        help="Input BigQuery table specified as: "
             "PROJECT.DATASET.TABLE.",
    )
    parser.add_argument(
        "--output_table_namespace",
        type=str,
        required=True,
        help="Output BigQuery namespace where event tables are located: PROJECT:DATASET.TABLE",
    )
    parser.add_argument(
        "--cluster",
        type=str,
        required=False,
        default="mainnet",
        help="Solana cluster being indexed: mainnet | devnet",
    )
    parser.add_argument(
        "--min_idl_version",
        type=int,
        required=False,
        default=0,
        help="Minimum IDL version to consider: int",
    )
    parser.add_argument(
        "--start_date",
        type=str,
        help="Start date to consider (inclusive) as: YYYY-MM-DD",
    )
    parser.add_argument(
        "--end_date",
        type=str,
        help="End date to consider (exclusive) as: YYYY-MM-DD",
    )
    known_args, remaining_args = parser.parse_known_args()

    run(
        input_table=known_args.input_table,
        output_table_namespace=known_args.output_table_namespace,
        cluster=known_args.cluster,
        min_idl_version=known_args.min_idl_version,
        start_date=known_args.start_date,
        end_date=known_args.end_date,
        beam_args=remaining_args,
    )


if __name__ == "__main__":
    main()
