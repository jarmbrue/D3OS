/*use bitfield::bitfield;


// pcie dvsec for Test Capability
//offset 04h
bitfield!{
    struct DVSEC_registers(MSB0[usize]);
    u16;
    vendor_id,_:15, 0;
    revision,_:19, 16;
    length,_:31, 20;
}
//offset 08h
bitfield!{
    struct DVSEC_registers_header_2(MSB0[usize]);
    u16;
    id,_:15, 0;
}
//offset 0ah
bitfield!{
    struct DVSEC_cxl_test_lock(MSB0[usize]);
    u16;
    vendor_id,_:0, 0;
    reserved,_:15, 1;
}
//offset 0ch
bitfield!{
    struct DVSEC_cxl_test_capability1(MSB0[usize]);
    u16;
    self_checking,_:0, 0;
    algorithm_1a,_:1, 1;
    algorithm_1b,_:2, 2;
    algorithm_2,_:3, 3;
    rd_curr,_:4, 4;
    rd_own,_:5, 5;
    rd_shared,_:6, 6;
    rd_any,_:7, 7;
    rd_own_no_data,_:8, 8;
    i_to_mwr,_:9, 9;
    mem_wr,_:10, 10;
    clflush,_:11, 11;
    clean_evict,_:12, 12;
    dirty_evict,_:13, 13;
    clean_evict_no_data,_:14, 14;
    wow_rinv,_:15, 15;
    wow_rinvf,_:16, 16;
    wrinv,_:17, 17;
    cache_flushed,_:18, 18;
    unexpected_completion,_:19, 19;
    completion_timeout_injection,_:20, 20;
    reserved,_:23, 21;
    configuration_size,_:31, 24;
}
//offset 10h
bitfield!{
    struct DVSEC_cxl_test_capability2(MSB0[usize]);
    u16;
    cache_size,_:13, 0;
    cache_size_units,_:15, 14;
}
//offset 14h
bitfield!{
    struct DVSEC_cxl_test_configuration_base_low(MSB0[usize]);
    u16;
    memory_space_indicator,_:0, 0;
    typ,_:2, 1;
    reserved,_:3, 3;
    base_low,_:31, 4;
}
//offset 18h
bitfield!{
    struct DVSEC_cxl_test_configuration_base_high(MSB0[usize]);
    u16;
    base_high,_:31, 0;
}


//device capabilities to support the test algorithms
//offset 00h
bitfield!{
    struct start_addr_1(MSB0[usize]);
    u16;
    start_addr,_:63, 0;
}

//offset 08h
bitfield!{
    struct write_back_addr_1(MSB0[usize]);
    u16;
    write_back_addr,_:63, 0;
}

//offset 10h
bitfield!{
    struct increment(MSB0[usize]);
    u16;
    address_increment,_:31, 0;
    set_offset,_:63, 32;
}

//offset 18h
bitfield!{
    struct pattern(MSB0[usize]);
    u16;
    pattern1,_:31, 0;
    pattern2,_:63, 32;
}

//offset 20h
bitfield!{
    struct byte_mask(MSB0[usize]);
    u16;
    byte_mask,_:63, 0;
}

//offset 28h
bitfield!{
    struct pattern_configuration(MSB0[usize]);
    u16;
    pattern_size,_:2, 0;
    pattern_parameter,_:3, 3;
    reserved,_:63, 4;
}

//offset 30h
bitfield!{
    struct algorithm_configuration(MSB0[usize]);
    u16;
    algorithm,_:2, 0;
    self_checking,_:3, 3;
    reserved_1,_:7, 4;
    number_of_addr_increments,_:15, 8;
    number_of_sets,_:23, 16;
    number_of_loops,_:31, 24;
    address_is_virtual,_:32, 32;
    protocol,_:35, 33;
    write_semantics_cache,_:39, 36;
    flush_cache,_:40, 40;
    execute_read_semantics_cache,_:43, 41;
    verify_read_semantics_cache,_:46, 44;
    reserved_2,_:63, 47;
}

//offset 38h
bitfield!{
    struct device_error_injection(MSB0[usize]);
    u16;
    unexpected_completion_injection,_:0, 0;
    unexpected_completion_injection_busy,_:1, 1;
    completer_timeout,_:2, 2;
    completer_timeout_injection_busy,_:3, 3;
    reserved,_:63, 4;
}





//debug capabilities in device. wird einfach angehangen
//offset 40h
bitfield!{
    struct error_log_1(MSB0[usize]);
    u16;
    expected_pattern,_:31, 0;
    observed_pattern,_:63, 32;
}

//offset 48h
bitfield!{
    struct error_log_2(MSB0[usize]);
    u16;
    expected_pattern,_:31, 0;
    observed_pattern,_:63, 32;
}

//offset 50h
bitfield!{
    struct error_log_3(MSB0[usize]);
    u16;
    byte_offset,_:7, 0;
    loop_num,_:15, 8;
    error_status,_:16, 16;
}

//offset 60h
bitfield!{
    struct event_ctrl(MSB0[usize]);
    u16;
    event_select,_:7, 0;
    sub_event_select,_:15, 8;
    reserved_1,_:16, 16;
    reset,_:17, 17;
    edge_detect,_:18, 18;
    reserved_2,_:63, 19;
}

//offset 68h
bitfield!{
    struct event_count(MSB0[usize]);
    u16;
    event_count,_:63, 0;
}

 */