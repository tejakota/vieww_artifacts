#include <stdio.h>
#include <stddef.h>
#include <clap/clap.h>
#define P(T) printf(#T " %zu\n", sizeof(T))
int main(void){
 P(clap_event_header_t); P(clap_event_note_t); printf("note.velocity %zu\n", offsetof(clap_event_note_t, velocity));
 P(clap_event_param_value_t); printf("param.value %zu\n", offsetof(clap_event_param_value_t, value));
 P(clap_event_transport_t); P(clap_event_midi_t); P(clap_param_info_t); P(clap_audio_buffer_t); P(clap_process_t);
 P(clap_plugin_t); P(clap_plugin_descriptor_t); P(clap_audio_port_info_t); P(clap_note_port_info_t); P(clap_host_t);
 P(clap_event_note_expression_t); P(clap_plugin_entry_t);
}
