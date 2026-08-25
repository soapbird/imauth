from google.protobuf import descriptor_pb2

from imauth.v1 import auth_pb2


def test_auth_event_reserves_removed_tag_seven():
    descriptor = descriptor_pb2.FileDescriptorProto.FromString(
        auth_pb2.DESCRIPTOR.serialized_pb
    )
    auth_event = next(
        message for message in descriptor.message_type if message.name == "AuthEvent"
    )

    assert 7 not in {field.number for field in auth_event.field}
    assert any(
        reserved.start == 7 and reserved.end == 8
        for reserved in auth_event.reserved_range
    )
