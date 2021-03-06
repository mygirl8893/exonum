// Copyright 2018 The Exonum Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(unsafe_code)]

use bit_vec::BitVec;
use byteorder::{ByteOrder, LittleEndian};
use chrono::{Duration, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use std::str::FromStr;

use super::{CheckedOffset, Field, Offset};
use blockchain::Block;
use crypto::{gen_keypair, hash};
use helpers::{user_agent, Height, Round, ValidatorId};
use messages::{
    BlockRequest, BlockResponse, Connect, Message, Precommit, Prevote, Propose, Status,
};

static VALIDATOR: ValidatorId = ValidatorId(65_123);
static HEIGHT: Height = Height(123_123_123);
static ROUND: Round = Round(321_321_312);

#[allow(dead_code)]
// This structures used to test deserialization,
// so we should ignore unused `new` method.
mod ignore_new {
    use crypto::Hash;
    encoding_struct! {
        struct Parent {
            child: Child,
        }
    }

    encoding_struct! {
        struct Child {
            child: &Hash,
        }
    }
}

use self::ignore_new::*;

#[test]
#[should_panic(expected = "Found error in check: UnexpectedlyShortPayload")]
fn test_zero_size_segment() {
    let buf = vec![
        8, 0, 0, 0, // not overlap
        0, 0, 0, 0, 0,
    ]; // but with zero size

    <Parent as Field>::check(&buf, 0.into(), 8.into(), 8.into()).expect("Found error in check");
}

#[test]
#[should_panic(expected = "Found error in check: UnexpectedlyShortPayload")]
fn test_incorrect_pointer() {
    let buf = vec![
        8, 0, 0, 0, // not overlap
        0, 0, 0, 0, 0,
    ]; // but with zero size

    <Parent as Field>::check(&buf, 0.into(), 8.into(), 8.into()).expect("Found error in check");
}

#[test]
#[should_panic(expected = "Found error in check: OffsetOverflow")]
fn test_read_overflow_arithmetic() {
    let pos = <u32>::max_value();
    let buf = vec![255; 0];

    <Vec<u8> as Field>::check(&buf, pos.into(), 8.into(), pos.into())
        .expect("Found error in check");
}

#[test]
fn test_bitvec() {
    let mut b = BitVec::from_elem(14, false);
    b.set(11, true);
    b.set(4, true);
    b.push(true);
    b.push(true);

    assert_write_check_read(b, 8);
}

#[test]
fn test_str_segment() {
    let mut buf = vec![0; 8];
    // spell-checker:disable-next
    let s = "test юникодной строчки efw_adqq ss/adfq";
    Field::write(&s, &mut buf, 0, 8);
    <&str as Field>::check(&buf, 0.into(), 8.into(), 8.into()).unwrap();

    let buf2 = buf.clone();
    <&str as Field>::check(&buf2, 0.into(), 8.into(), 8.into()).unwrap();
    let s2: &str = unsafe { Field::read(&buf2, 0, 8) };
    assert_eq!(s2, s);
}

#[test]
fn test_vec_segment() {
    let dat = vec![1u8, 2, 3, 5, 10];
    assert_write_check_read(dat, 8);
}

#[test]
fn test_u16_segment() {
    let dat = vec![1u16, 3, 10, 15, 23, 4, 45];
    assert_write_check_read(dat, 8);
}

#[test]
fn test_i16_segment() {
    let dat = vec![1i16, 3, 10, 15, 23, 4, 45];
    assert_write_check_read(dat, 8);
}

#[test]
fn test_u32_segment() {
    let dat = vec![1u32, 3, 10, 15, 23, 4, 45];
    assert_write_check_read(dat, 8);
}

#[test]
fn test_i32_segment() {
    let dat = vec![1i32, 3, 10, 15, 23, 4, 45];
    assert_write_check_read(dat, 8);
}

#[test]
fn test_u64_segment() {
    let dat = vec![1u64, 3, 10, 15, 23, 4, 45];
    assert_write_check_read(dat, 8);
}

#[test]
fn test_i64_segment() {
    let dat = vec![1i64, 3, 10, 15, 23, 4, 45];
    assert_write_check_read(dat, 8);
}

#[test]
fn test_uuid_segment() {
    let uuid = Uuid::nil();
    assert_write_check_read(uuid, 16);

    let uuid = Uuid::parse_str("936DA01F9ABD4d9d80C702AF85C822A8").unwrap();
    assert_write_check_read(uuid, 16);
}

#[test]
fn test_decimal() {
    let decimal = Decimal::new(202, 2);
    assert_write_check_read(decimal, 16);
    let pi = Decimal::from_str("3.1415926535897932384626433832").unwrap();
    assert_write_check_read(pi, 16);
}

#[test]
fn test_check_invalid_duration_too_big() {
    let secs = i64::max_value();
    let nanos = i32::max_value();

    expect_duration_check_error(secs, nanos);
}

#[test]
fn test_check_invalid_duration_too_low() {
    let secs = i64::min_value();
    let nanos = i32::min_value();

    expect_duration_check_error(secs, nanos);
}

#[test]
fn test_check_invalid_duration_wrong_representation() {
    // Different signs of values are forbidden to avoid multiple representations of same duration.
    let secs = 1;
    let nanos = -1;
    expect_duration_check_error(secs, nanos);

    let secs = -1;
    let nanos = 1;
    expect_duration_check_error(secs, nanos);

    // Amount of nanoseconds greater of equal to one second is forbidden.
    let secs = 0;
    let nanos = 1_000_000_000;
    expect_duration_check_error(secs, nanos);

    let secs = 0;
    let nanos = -1_000_000_000;
    expect_duration_check_error(secs, nanos);
}

#[test]
fn test_check_valid_duration_that_exceeds_max_value() {
    let secs: i64 = Duration::max_value().num_seconds();
    let max_duration_nanos = Duration::max_value() - Duration::seconds(secs);
    let mut nanos: i32 = max_duration_nanos.num_nanoseconds().unwrap() as i32;
    nanos += 1;

    expect_duration_check_error(secs, nanos);
}

fn expect_duration_check_error(secs: i64, nanos: i32) {
    // Size of duration is sizeof(i64) + sizeof(i32).
    let header_size = 12;

    let mut raw_duration: Vec<u8> = vec![0; header_size];

    LittleEndian::write_i64(&mut raw_duration[0 as usize..8 as usize], secs);
    LittleEndian::write_i32(&mut raw_duration[8 as usize..header_size as usize], nanos);

    let start_offset = CheckedOffset::new(0 as Offset);
    let end_offset = CheckedOffset::new(header_size as Offset);
    <Duration as Field>::check(&raw_duration, start_offset, end_offset, end_offset)
        .expect_err("Check should return DurationOverflow error for incorrect buffer");
}

#[test]
fn test_duration_segment() {
    // Size of duration is sizeof(i64) + sizeof(i32).
    let header_size = 12;

    let zero_duration = Duration::zero();
    assert_write_check_read(zero_duration, header_size);

    let max_duration = Duration::max_value();
    assert_write_check_read(max_duration, header_size);

    let min_duration = Duration::min_value();
    assert_write_check_read(min_duration, header_size);

    // Composite durations for all correct combination of values sign.
    let durations = [(10, 0), (0, 10), (10, 10), (0, -10), (-10, 0), (-10, -10)];

    for value in durations.iter() {
        let duration = Duration::seconds(value.0) + Duration::nanoseconds(value.1);
        assert_write_check_read(duration, header_size);
    }
}

#[test]
fn test_byte_array() {
    let mut buf = vec![255; 8];
    let arr = [2u8, 5, 2, 3, 56, 3];

    Field::write(&arr.as_ref(), &mut buf, 0, 8);
    <&[u8] as Field>::check(&buf, 0.into(), 8.into(), 8.into()).unwrap();

    let buf2 = buf.clone();
    <&[u8] as Field>::check(&buf2, 0.into(), 8.into(), 8.into()).unwrap();
    let dat2: &[u8] = unsafe { Field::read(&buf2, 0, 8) };
    assert_eq!(dat2, arr);
    assert_eq!(buf.len(), 8 + arr.len());
}

#[test]
fn test_segments_of_arrays() {
    let mut buf = vec![255; 64];
    let v1 = [1u8, 2, 3];
    let v2 = [1u8, 3];
    let v3 = [2u8, 5, 2, 3, 56, 3];

    let dat = vec![v1.as_ref(), v2.as_ref(), v3.as_ref()];
    Field::write(&dat, &mut buf, 48, 56);
    <Vec<&[u8]> as Field>::check(&buf, 48.into(), 56.into(), 64.into()).unwrap();

    let buf2 = buf.clone();
    <Vec<&[u8]> as Field>::check(&buf2, 48.into(), 56.into(), 64.into()).unwrap();
    let dat2: Vec<&[u8]> = unsafe { Field::read(&buf2, 48, 56) };
    assert_eq!(dat2, dat);
    //48 spaces + 8 segment of vec + 8 spaces = 64 +
    // + v1_segment + v2_segment + v3_segment +
    // + v1_body + v2_body + v3_body
    assert_eq!(buf.len(), 64 + v1.len() + v2.len() + v3.len() + 3 * 8);
}

fn assert_write_check_read<T>(input: T, header_size: Offset)
where
    T: for<'r> Field<'r> + PartialEq + ::std::fmt::Debug,
{
    let mut buffer = vec![0; header_size as usize];
    Field::write(&input, &mut buffer, 0, header_size);
    <T as Field>::check(&buffer, 0.into(), header_size.into(), header_size.into()).unwrap();
    let new_buffer = buffer.clone();
    //clear buffer
    let len = buffer.len();
    buffer.clear();
    //and fill old buffer with zeros
    buffer.resize(len, 0);

    <T as Field>::check(
        &new_buffer,
        0.into(),
        header_size.into(),
        header_size.into(),
    ).unwrap();
    let output = unsafe { Field::read(&new_buffer, 0, header_size) };
    assert_eq!(input, output);
}

#[test]
fn test_segments_of_raw_buffers() {
    let buf = vec![255_u8; 1];

    let input = vec![buf.clone(), buf.clone(), buf.clone()];
    assert_write_check_read(input, 8);
}

#[test]
fn test_empty_segments() {
    let dat: Vec<Vec<u8>> = vec![];
    assert_write_check_read(dat, 8);
}

#[test]
fn test_segments_of_status_messages() {
    let m1 = Status::new(Height(2), &hash(&[]));
    let m2 = Status::new(Height(4), &hash(&[1]));
    let m3 = Status::new(Height(5), &hash(&[3]));

    let dat = vec![m1, m2, m3];
    assert_write_check_read(dat, 8);
}

fn test_connect(addr: &str) {
    let time = Utc::now();
    let (public_key, secret_key) = gen_keypair();

    // write
    let connect = Connect::new(addr, time, &user_agent::get());
    let connect = Message::concrete(connect, public_key, &secret_key);
    // read
    assert_eq!(connect.author(), public_key);
    assert_eq!(connect.pub_addr(), addr);
    assert_eq!(connect.time(), time);
}

#[test]
fn test_connect_ipv4() {
    test_connect("18.34.3.4:7777");
}

#[test]
fn test_connect_ipv6() {
    test_connect("[::1]:7777");
}

#[test]
fn test_propose() {
    let prev_hash = hash(&[1, 2, 3]);
    let txs = vec![hash(&[1]), hash(&[2]), hash(&[2])];

    // write
    let propose = Propose::new(VALIDATOR, HEIGHT, ROUND, &prev_hash, &txs);
    // read
    assert_eq!(propose.validator(), VALIDATOR);
    assert_eq!(propose.height(), HEIGHT);
    assert_eq!(propose.round(), ROUND);
    assert_eq!(propose.prev_hash(), &prev_hash);
    assert_eq!(propose.transactions().len(), 3);
    assert_eq!(propose.transactions()[0], txs[0]);
    assert_eq!(propose.transactions()[1], txs[1]);
    assert_eq!(propose.transactions()[2], txs[2]);
}

#[test]
fn test_prevote() {
    let propose_hash = hash(&[1, 2, 3]);
    let locked_round = Round(654_345);

    // write
    let prevote = Prevote::new(VALIDATOR, HEIGHT, ROUND, &propose_hash, locked_round);
    // read
    assert_eq!(prevote.validator(), VALIDATOR);
    assert_eq!(prevote.height(), HEIGHT);
    assert_eq!(prevote.round(), ROUND);
    assert_eq!(prevote.propose_hash(), &propose_hash);
    assert_eq!(prevote.locked_round(), locked_round);
}

#[test]
fn test_precommit() {
    let propose_hash = hash(&[1, 2, 3]);
    let block_hash = hash(&[3, 2, 1]);
    let time = Utc::now();

    // write
    let precommit = Precommit::new(VALIDATOR, HEIGHT, ROUND, &propose_hash, &block_hash, time);
    // read
    assert_eq!(precommit.validator(), VALIDATOR);
    assert_eq!(precommit.height(), HEIGHT);
    assert_eq!(precommit.round(), ROUND);
    assert_eq!(precommit.propose_hash(), &propose_hash);
    assert_eq!(precommit.block_hash(), &block_hash);
    assert_eq!(precommit.time(), time);
    let json_str = ::serde_json::to_string(&precommit).unwrap();
    assert!(json_str.len() > 0);
    let precommit1: Precommit = ::serde_json::from_str(&json_str).unwrap();
    assert_eq!(precommit, precommit1);
}

#[test]
fn test_status() {
    let last_hash = hash(&[3, 2, 1]);

    // write
    let commit = Status::new(HEIGHT, &last_hash);
    // read
    assert_eq!(commit.height(), HEIGHT);
    assert_eq!(commit.last_hash(), &last_hash);
}

#[test]
fn test_empty_block() {
    let (pub_key, _secret_key) = gen_keypair();

    let content = Block::new(
        ValidatorId::zero(),
        Height(200),
        1,
        &hash(&[1]),
        &hash(&[2]),
        &hash(&[3]),
    );

    let precommits = Vec::new();
    let transactions = Vec::new();
    let block = BlockResponse::new(&pub_key, content.clone(), precommits.clone(), &transactions);

    assert_eq!(block.to(), &pub_key);
    assert_eq!(block.block(), content);
    assert_eq!(block.precommits(), precommits);
    assert_eq!(block.transactions().to_vec(), transactions);
}

#[test]
fn test_request_block() {
    let (public_key, _secret_key) = gen_keypair();

    // write
    let request = BlockRequest::new(&public_key, Height(1));
    // read
    assert_eq!(request.height(), Height(1));
    assert_eq!(request.to(), &public_key);
}

#[test]
fn test_correct_encoding_struct() {
    encoding_struct! {
        struct NoFields {
        }
    }
    drop(NoFields::new());

    encoding_struct! {
        struct OneField {

            one: u64,
        }
    }
    drop(OneField::new(0));

    encoding_struct! {
        struct TwoFields {

            one: u32,
            two: u32,
        }
    }
    drop(TwoFields::new(0, 0));

    encoding_struct! {
        struct ThreeFields {

            one: u16,
            two: u16,
            three: u32,
        }
    }
    drop(ThreeFields::new(0, 0, 0));
}

#[test]
fn test_option_serialization_roundtrip() {
    use encoding::serialize::json::ExonumJson;

    encoding_struct!(struct Foo {
        opt1: Option<u32>,
        opt2: Option<Bar>,
    });

    encoding_struct!(struct Bar {
        name: &str,
    });

    let opts = vec![
        (Foo::new(None, None), json!({"opt1": null, "opt2": null})),
        (
            Foo::new(Some(0xFFFFFFFF), None),
            json!({"opt1": 4294967295_u32, "opt2": null}),
        ),
        (
            Foo::new(Some(0xFFFFFFFF), Some(Bar::new("Baz"))),
            json!({"opt1": 4294967295_u32, "opt2": {"name": "Baz"}}),
        ),
        (
            Foo::new(None, Some(Bar::new("Baz"))),
            json!({"opt1": null, "opt2": {"name": "Baz"}}),
        ),
    ];

    for (x, json_x) in opts.iter() {
        let mut buffer = vec![0; Foo::field_size() as usize];
        let from: Offset = 0;
        let to: Offset = Foo::field_size();
        let checked_from = CheckedOffset::new(from);
        let checked_to = CheckedOffset::new(to);

        let serialized = x.serialize_field().unwrap();
        assert_eq!(*json_x, serialized);
        Foo::deserialize_field(&serialized, &mut buffer, from, to).unwrap();
        Foo::check(&buffer, checked_from, checked_to, checked_to).unwrap();
        let foo = unsafe { Foo::read(&buffer, from, to) };
        assert_eq!(*x, foo);
    }
}
