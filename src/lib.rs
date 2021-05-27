extern crate cpython;
extern crate ffmpeg_next as ffmpeg;
use std::fs;
use std::path::Path;
use ffmpeg::{codec, encoder, format, log, media, Rational};
use cpython::{PyResult, Python, py_module_initializer, py_fn};

py_module_initializer!(libcut_ts, |py, m| {
    m.add(py, "__doc__", "Cut media file")?;
    m.add(py, "cut_ts_iframe", py_fn!(py, call_cut_ts(input_file: &str, output_file:&str, 
                                                      skip:f64, dur:f64)))?;

    //TODO: implement frame base
    m.add(py, "cut_ts_frame", py_fn!(py, call_cut_ts(input_file: &str, output_file:&str, 
                                                      skip:f64, dur:f64)))?;
    m.add(py, "split_audios", py_fn!(py, split_audios(input_file: &str, base_dir: &str,
                                                      seg_name: &str)))?;
    m.add(py, "media_info", py_fn!(py, media_info(file_name: &str)))?;
    Ok(())
});

fn split_audios(_py: Python, input_file: &str, base_dir: &str, seg_name: &str) -> PyResult<i32> {
    if let Err(error) = ffmpeg::init() {
        println!("Can't init ffmpeg {}", error);  
        return Ok(0)
    }
    let mut ictx = match format::input(&input_file) {
                    Ok(ctx) => ctx,
                    Err(error) => { 
                        println!("Can't open {} {:?}",
                        input_file, error); return Ok(0);}
    };
    
    let mut out_names = Vec::<String>::new();
    let mut octxs = Vec::<format::context::Output>::new();
    let mut stream_mapping = vec![0i32; ictx.nb_streams() as _];
    let mut ist_time_bases = vec![Rational(0, 1); ictx.nb_streams() as _];
    let mut ost_index = 0;
    
    for (ist_index, ist) in ictx.streams().enumerate() {
        let ist_medium = ist.codec().medium();
        if ist_medium != media::Type::Audio
        {
            stream_mapping[ist_index] = -1;
            continue;
        }
        
        let out_dir = base_dir.to_owned() + &"/audio_".to_owned() + &ost_index.to_string();
        if !Path::new(&out_dir).exists() {
            println!("Create {}", &out_dir);  
            fs::create_dir(&out_dir).unwrap_or_default();
        }
        let out_name = out_dir + "/" + seg_name + ".tmp.ts";
        let mut ocx = match format::output(&out_name) {
                    Ok(ctx) => ctx,
                    Err(error) => { 
                        println!("Can't open {} {:?}",
                        input_file, error); return Ok(0);}
            };
        out_names.push(out_name);
        stream_mapping[ist_index] = ost_index;
        ist_time_bases[ist_index] = ist.time_base();
        let mut ost = match ocx.add_stream(encoder::find(codec::Id::None)){
            Ok(s) => s,
            Err(_) => continue
        };
        ost.set_parameters(ist.parameters());
        // We need to set codec_tag to 0 lest we run into incompatible codec tag
        // issues when muxing into a different container format. Unfortunately
        // there's no high level API to do this (yet).
        unsafe {
            (*ost.parameters().as_mut_ptr()).codec_tag = 0;
        }
        &octxs.push(ocx);
        ost_index += 1;
    }
    let audio_num = ost_index;
    for octx in &mut octxs {
        octx.set_metadata(ictx.metadata().to_owned());
        if let Err(_) = octx.write_header() {
            return Ok(0)
        }
    }

    for (stream, mut packet) in ictx.packets() {
        let ist_index = stream.index();
        let ost_index = stream_mapping[ist_index];
        if ost_index < 0 {
            continue;
        }

        //println!("audio in {} out {}",ist_index, ost_index);  
        let ost = octxs[ost_index as usize].stream(0).unwrap();
        packet.rescale_ts(ist_time_bases[ist_index], ost.time_base());
        packet.set_position(-1);
        packet.set_stream(0);
        packet.write_interleaved(&mut octxs[ost_index as usize]).unwrap_or_default();
    }
    for octx in &mut octxs {
        octx.write_trailer().unwrap();
    }
    for out_name in &out_names {
        let new_name = out_name.replace(".tmp.ts", "");
        std::fs::rename(out_name, new_name).unwrap();
    }
    Ok(audio_num)
}

fn call_cut_ts(_py: Python, input_file: &str, output_file: &str, skip: f64, dur: f64) -> 
                                                                    PyResult<bool> {
    if cut_ts_iframe( String::from(input_file), String::from(output_file), skip, dur) {
        Ok(true)
    } else {
        Ok(false)
    }
}
fn cut_ts_iframe( input_file: String, output_file: String, skip: f64, dur:f64) -> bool {

    if let Err(error) = ffmpeg::init() {
        println!("Can't init ffmpeg {}", error);  
        return false;
    }
    log::set_level(log::Level::Warning);

    let mut ictx = match format::input(&input_file) {
                    Ok(ctx) => ctx,
                    Err(error) => { println!("Can't open {} {:?}",
                                             input_file, error); return false;}
    };
    let mut octx = match format::output(&output_file) {
                    Ok(ctx) => ctx,
                    Err(error) => { println!("Can't open {} {:?}",
                                             input_file, error); return false;}
    };

    let mut stream_mapping = vec![0i32; ictx.nb_streams() as _];
    let mut ist_time_bases = vec![Rational(0, 1); ictx.nb_streams() as _];
    let mut ost_index = 0;
    let mut video_id = 0;
    
    for (ist_index, ist) in ictx.streams().enumerate() {
        let ist_medium = ist.codec().medium();
        if ist_medium != media::Type::Audio
            && ist_medium != media::Type::Video
            && ist_medium != media::Type::Subtitle
        {
            stream_mapping[ist_index] = -1;
            continue;
        }
        if ist_medium == media::Type::Video {
            video_id = ist_index;
        }
        stream_mapping[ist_index] = ost_index;
        ist_time_bases[ist_index] = ist.time_base();
        ost_index += 1;
        let mut ost = match octx.add_stream(encoder::find(codec::Id::None)){
            Ok(s) => s,
            Err(_) => continue
        };
        /* If you want to merge segment with other parts, 
         * you should set same stream id in output. for this resone you should add 
         * this fun to ffmpeg_next code in file : stream_mut.rs an StreamMut :
            pub fn set_id(&mut self, id: i32) {
                unsafe { (*self.as_mut_ptr()).id = id; }
            }
         */
        // Uncomment if you change in ffmpeg_next code!
        // ost.set_id(ist.id());  

        ost.set_parameters(ist.parameters());
        // We need to set codec_tag to 0 lest we run into incompatible codec tag
        // issues when muxing into a different container format. Unfortunately
        // there's no high level API to do this (yet).
        unsafe {
            (*ost.parameters().as_mut_ptr()).codec_tag = 0;
        }
    }

    octx.set_metadata(ictx.metadata().to_owned());

    if let Err(_) = octx.write_header() {
        return false;
    }

    let mut out_pkt_count = 0;
    let mut skip_pkt_count = 0;
    let mut before_keyframe_and_skeep = 1;
    let mut start_time = 0f64;
    let mut ts_start_time = 0f64;
    let mut video_pkt_time = 0f64;

    for (stream, mut packet) in ictx.packets() {
        let ist_index = stream.index();
        let ost_index = stream_mapping[ist_index];
        if ost_index < 0 {
            continue;
        }
        if ist_index == video_id {
            let pkt_base: f64 = ist_time_bases[ist_index].into();
            let pkt_pts: f64 = packet.pts().unwrap_or(0) as f64;
            video_pkt_time = pkt_pts * pkt_base; 
            if ts_start_time == 0.0 { 
                ts_start_time = video_pkt_time; 
            }
            if before_keyframe_and_skeep  != 0 {
                if (video_pkt_time - ts_start_time < skip ) ||
                    !packet.is_key() {
                        before_keyframe_and_skeep += 1;
                        continue;
                    }
                skip_pkt_count = before_keyframe_and_skeep;
                before_keyframe_and_skeep = 0;
                start_time = video_pkt_time; 
            }
            out_pkt_count += 1;
        }
        if before_keyframe_and_skeep != 0 {
            continue;
        }
        if ist_index == video_id && dur > 0.0 {
            if video_pkt_time - start_time > dur {
                break;
            }
        }

        let ost = octx.stream(ost_index as _).unwrap();
        packet.rescale_ts(ist_time_bases[ist_index], ost.time_base());
        packet.set_position(-1);
        packet.set_stream(ost_index as _);
        packet.write_interleaved(&mut octx).unwrap_or_default();
    }
    println!("Ts start {:3.3}  out start {:3.3} Skip pkt {} Out pkt {}",
             ts_start_time , start_time,
             skip_pkt_count-1, out_pkt_count);  
    octx.write_trailer().unwrap();
    true
}
fn media_info(_py: Python, file_name: &str)  -> PyResult<String> {
    match ffmpeg::format::input(&String::from(file_name)) {
        Ok(context) => {
            for (k, v) in context.metadata().iter() {
                println!("{}: {}", k, v);
            }

            if let Some(stream) = context.streams().best(ffmpeg::media::Type::Video) {
                println!("Best video stream index: {}", stream.index());
            }

            if let Some(stream) = context.streams().best(ffmpeg::media::Type::Audio) {
                println!("Best audio stream index: {}", stream.index());
            }

            if let Some(stream) = context.streams().best(ffmpeg::media::Type::Subtitle) {
                println!("Best subtitle stream index: {}", stream.index());
            }

            println!(
                "duration (seconds): {:.2}",
                context.duration() as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE)
            );
        },
        Err(error) => println!("error: {}", error),
    };
    Ok("Ok".to_owned())
}
