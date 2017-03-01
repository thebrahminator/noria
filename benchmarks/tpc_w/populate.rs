use std::io::{BufRead, BufReader};
use std::fs::File;
use std::str::FromStr;
use std::time;

use distributary::Token;
use super::Backend;

pub fn populate_addresses(backend: &Backend, data_location: &str) {
    let addresses_putter = backend.g.get_mutator(backend.r.node_addr_for("address").unwrap());

    let f = File::open(format!("{}/addresses.tsv", data_location)).unwrap();
    let mut reader = BufReader::new(f);

    let mut s = String::new();
    let start = time::Instant::now();
    let mut i = 0;
    while reader.read_line(&mut s).unwrap() > 0 {
        {
            let fields: Vec<&str> = s.split("\t").collect();
            let addr_id = i32::from_str(fields[0]).unwrap();
            let addr_street1 = fields[1];
            let addr_street2 = fields[2];
            let addr_city = fields[3];
            let addr_state = fields[4];
            let addr_zip = fields[5];
            let addr_co_id = fields[6];
            addresses_putter.put(vec![addr_id.into(),
                                      addr_street1.into(),
                                      addr_street2.into(),
                                      addr_city.into(),
                                      addr_state.into(),
                                      addr_zip.into(),
                                      addr_co_id.into()]);
        }
        i += 1;
        s.clear();
    }
    println!("Wrote {} addresses in {:.2}s!",
             i,
             start.elapsed().as_secs());
}

pub fn populate_authors(backend: &Backend, data_location: &str) {
    let author_putter = backend.g.get_mutator(backend.r.node_addr_for("author").unwrap());

    let f = File::open(format!("{}/authors.tsv", data_location)).unwrap();
    let mut reader = BufReader::new(f);

    let mut s = String::new();
    let start = time::Instant::now();
    let mut i = 0;
    while reader.read_line(&mut s).unwrap() > 0 {
        {
            let fields: Vec<&str> = s.split("\t").collect();
            let a_id = i32::from_str(fields[0]).unwrap();
            let a_fname = fields[1];
            let a_lname = fields[2];
            let a_mname = fields[3];
            let a_dob = fields[4];
            let a_bio = fields[5];
            author_putter.put(vec![a_id.into(),
                                   a_fname.into(),
                                   a_lname.into(),
                                   a_mname.into(),
                                   a_dob.into(),
                                   a_bio.into()]);
        }
        i += 1;
        s.clear();
    }
    println!("Wrote {} authors in {:.2}s!", i, start.elapsed().as_secs());
}

pub fn populate_countries(backend: &Backend, data_location: &str) {
    let country_putter = backend.g.get_mutator(backend.r.node_addr_for("country").unwrap());

    let f = File::open(format!("{}/countries.tsv", data_location)).unwrap();
    let mut reader = BufReader::new(f);

    let mut s = String::new();
    let start = time::Instant::now();
    let mut i = 0;
    while reader.read_line(&mut s).unwrap() > 0 {
        {
            let fields: Vec<&str> = s.split("\t").collect();
            let co_id = i32::from_str(fields[0]).unwrap();
            let co_name = fields[1];
            let co_exchange = fields[2]; // XXX(malte): DataType doesn't support floats
            let co_currency = fields[3];
            country_putter.put(vec![co_id.into(),
                                    co_name.into(),
                                    co_exchange.into(),
                                    co_currency.into()]);
        }
        i += 1;
        s.clear();
    }
    println!("Wrote {} countries in {:.2}s!",
             i,
             start.elapsed().as_secs());
}

pub fn populate_customers(backend: &Backend, data_location: &str) {
    let customers_putter = backend.g.get_mutator(backend.r.node_addr_for("customer").unwrap());

    let f = File::open(format!("{}/customers.tsv", data_location)).unwrap();
    let mut reader = BufReader::new(f);

    let mut s = String::new();
    let start = time::Instant::now();
    let mut i = 0;
    while reader.read_line(&mut s).unwrap() > 0 {
        {
            let fields: Vec<&str> = s.split("\t").collect();
            let c_id = i32::from_str(fields[0]).unwrap();
            let c_uname = fields[1];
            let c_passwd = fields[2];
            let c_fname = fields[3];
            let c_lname = fields[4];
            let c_addr_id = i32::from_str(fields[5]).unwrap();
            let c_phone = fields[6];
            let c_email = fields[7];
            let c_since = fields[8];
            let c_last_login = fields[9];
            let c_login = fields[10];
            let c_expiration = fields[11];
            let c_discount = fields[12]; // XXX(malte): DataType doesn't support floats
            let c_balance = fields[13]; // XXX(malte): DataType doesn't support floats
            let c_ytd_pmt = fields[14]; // XXX(malte): DataType doesn't support floats
            let c_birthdate = fields[15];
            let c_data = fields[16];
            customers_putter.put(vec![c_id.into(),
                                      c_uname.into(),
                                      c_passwd.into(),
                                      c_fname.into(),
                                      c_lname.into(),
                                      c_addr_id.into(),
                                      c_phone.into(),
                                      c_email.into(),
                                      c_since.into(),
                                      c_last_login.into(),
                                      c_login.into(),
                                      c_expiration.into(),
                                      c_discount.into(),
                                      c_balance.into(),
                                      c_ytd_pmt.into(),
                                      c_birthdate.into(),
                                      c_data.into()]);
        }
        i += 1;
        s.clear();
    }
    println!("Wrote {} customers in {:.2}s!",
             i,
             start.elapsed().as_secs());
}


pub fn populate_orders(backend: &Backend, data_location: &str) {
    let order_putter = backend.g.get_mutator(backend.r.node_addr_for("orders").unwrap());

    let f = File::open(format!("{}/orders.tsv", data_location)).unwrap();
    let mut reader = BufReader::new(f);

    let mut s = String::new();
    let start = time::Instant::now();
    let mut i = 0;
    while reader.read_line(&mut s).unwrap() > 0 {
        {
            let fields: Vec<&str> = s.split("\t").collect();
            let o_id = i32::from_str(fields[0]).unwrap();
            let o_c_id = i32::from_str(fields[1]).unwrap();
            let o_date = fields[2];
            let o_sub_total = fields[3]; // XXX(malte): DataType doesn't support floats
            let o_tax = fields[4]; // XXX(malte): DataType doesn't support floats
            let o_total = fields[5]; // XXX(malte): DataType doesn't support floats
            let o_ship_type = fields[6];
            let o_ship_date = fields[7];
            let o_bill_addr_id = i32::from_str(fields[8]).unwrap();
            let o_ship_addr_id = i32::from_str(fields[9]).unwrap();
            let o_status = fields[10];

            order_putter.put(vec![o_id.into(),
                                  o_c_id.into(),
                                  o_date.into(),
                                  o_sub_total.into(),
                                  o_tax.into(),
                                  o_total.into(),
                                  o_ship_type.into(),
                                  o_ship_date.into(),
                                  o_bill_addr_id.into(),
                                  o_ship_addr_id.into(),
                                  o_status.into()]);
        }
        i += 1;
        s.clear();
    }
    println!("Wrote {} orders in {:.2}s!", i, start.elapsed().as_secs());
}
