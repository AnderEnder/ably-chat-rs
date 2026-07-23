# Message

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**serial** | **String** | The unique identifier of the message. | 
**version** | [**models::MessageVersion**](MessageVersion.md) |  | 
**text** | **String** | The text content of the message. | 
**client_id** | **String** | The client ID of the user who created the message. | 
**action** | [**models::MessageAction**](MessageAction.md) |  | 
**metadata** | **std::collections::HashMap<String, serde_json::Value>** | Arbitrary user-defined metadata. Not interpreted or validated by Ably; treat as untrusted input when reading.  | 
**headers** | **std::collections::HashMap<String, String>** | Arbitrary user-defined string headers, carried as Ably message `extras.headers`. Not interpreted or validated by Ably.  | 
**timestamp** | **i64** | Milliseconds since the Unix epoch at which the message was created. | 
**user_claim** | Option<**String**> | User claim attached by the server when the publishing token contained a matching `ably.room.<roomName>` claim. Present only in that case.  | [optional]
**reactions** | Option<[**models::MessageReactions**](MessageReactions.md)> |  | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


