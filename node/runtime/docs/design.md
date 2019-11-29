# Design Document

### Query Interface

```
// all available tokens to fetch from
get_all_tokens -> [sym1, sym2, sym3]

// all remote src we fetched from
get_all_remote_srcs -> [remote_src1, remote_src2]

// get the latest `sym` price from our aggregated price stream
get_current_price(sym) -> (Price, last_update)

//get the series from our aggregated price stream
//  param2: 1 - 100
get_price_series(sym, 50) -> [(Price1, last_update1), (Price2, last_update2), (Price3, last_update3)]

// the total len of a sym for our aggregated price stream
get_price_series_len(sym) -> num

get_price_src_series(sym, remote_src, 50) -> [(...), (...), (...)]
get_price_src_series_len(sym, remote_src) -> num
```
